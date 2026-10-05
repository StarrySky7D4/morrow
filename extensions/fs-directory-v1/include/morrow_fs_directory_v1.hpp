#ifndef MORROW_FS_DIRECTORY_V1_HPP
#define MORROW_FS_DIRECTORY_V1_HPP
#include "morrow_fs_directory_v1.h"
#include <array>
#include <algorithm>
#include <cstddef>
#include <memory>
#include <optional>
#include <utility>
#include <vector>
namespace morrow { namespace fs_directory_v1 {
enum class name_encoding : uint32_t { utf8=MFD_UTF8_NAME,utf16le=MFD_UTF16LE_NAME };
enum class entry_kind : uint32_t { file=MFD_FILE,directory=MFD_DIRECTORY,other=MFD_OTHER };
struct entry {
  std::array<uint8_t,32> entry_id{};
  std::vector<uint8_t> name;
  name_encoding encoding=name_encoding::utf8;
  entry_kind kind=entry_kind::file;
  std::optional<uint64_t> logical_length;
};
class directory_state;
class page {
  struct deleter {void operator()(mfd_page_v1* p)const noexcept{mfd_page_v1_free(p);}};
  std::unique_ptr<mfd_page_v1,deleter> owner_;
  friend class directory_state;
public:
  page()noexcept=default;
  page(const page&)=delete;
  page& operator=(const page&)=delete;
  page(page&&)noexcept=default;
  page& operator=(page&&)noexcept=default;
  bool has_value()const noexcept{return bool(owner_);}
  uint32_t decode(const uint8_t* data,std::size_t length){
    if(length>MFD_MAX_ENVELOPE_BYTES)return MFD_LIMIT;
    mfd_page_v1* next=nullptr;auto code=mfd_page_v1_decode(data,static_cast<uint32_t>(length),&next);
    if(code==MFD_OK)owner_.reset(next);return code;
  }
  uint32_t decode(const std::vector<uint8_t>& wire){return decode(wire.data(),wire.size());}
  uint32_t assign(const std::array<uint8_t,32>& epoch,uint64_t sequence,const std::vector<entry>& entries,bool terminal){
    if(entries.size()>MFD_MAX_ENTRIES_PER_PAGE)return MFD_LIMIT;
    std::size_t total=0;for(const auto& e:entries){if(e.name.size()>MFD_MAX_NAME_BYTES_PER_PAGE-total)return MFD_LIMIT;total+=e.name.size();}
    std::vector<mfd_entry_view_v1> refs;refs.reserve(entries.size());
    for(const auto& e:entries){mfd_entry_view_v1 r{};std::copy(e.entry_id.begin(),e.entry_id.end(),r.entry_id);r.name=e.name.data();r.name_length=static_cast<uint32_t>(e.name.size());r.encoding=static_cast<uint32_t>(e.encoding);r.kind=static_cast<uint32_t>(e.kind);r.has_logical_length=e.logical_length?1u:0u;r.logical_length=e.logical_length.value_or(0);refs.push_back(r);}
    mfd_page_view_v1 v{};v.abi_version=MFD_ABI_VERSION;v.struct_size=sizeof v;std::copy(epoch.begin(),epoch.end(),v.selection_epoch);v.page_sequence=sequence;v.entry_count=static_cast<uint32_t>(refs.size());v.terminal=terminal?1u:0u;v.entries=refs.data();
    mfd_page_v1* next=nullptr;auto code=mfd_page_v1_create(&v,sizeof v,&next);if(code==MFD_OK)owner_.reset(next);return code;
  }
  uint32_t view(mfd_page_view_v1& out)const{return mfd_page_v1_view(owner_.get(),&out,sizeof out);}
  uint32_t entry_view(uint32_t index,mfd_entry_view_v1& out)const{return mfd_page_v1_entry(owner_.get(),index,&out,sizeof out);}
  uint32_t encode(std::vector<uint8_t>& out)const{
    std::vector<uint8_t> next(MFD_MAX_ENVELOPE_BYTES);uint32_t length=0;auto code=mfd_page_v1_encode(owner_.get(),next.data(),static_cast<uint32_t>(next.size()),&length);
    if(code==MFD_OK){next.resize(length);out.swap(next);}return code;
  }
};
inline mfd_state_limits_v1 default_limits()noexcept{return {MFD_ABI_VERSION,sizeof(mfd_state_limits_v1),MFD_MAX_STATE_ENTRIES,MFD_MAX_STATE_PAGES,MFD_MAX_STATE_NAME_BYTES,MFD_MAX_STATE_WIRE_BYTES,0};}
class directory_state {
  struct deleter {void operator()(mfd_state_v1* p)const noexcept{mfd_state_v1_free(p);}};
  std::unique_ptr<mfd_state_v1,deleter> owner_;
public:
  directory_state()noexcept=default;
  directory_state(const directory_state&)=delete;
  directory_state& operator=(const directory_state&)=delete;
  directory_state(directory_state&&)noexcept=default;
  directory_state& operator=(directory_state&&)noexcept=default;
  bool has_value()const noexcept{return bool(owner_);}
  uint32_t create(const std::array<uint8_t,32>& epoch,const mfd_state_limits_v1& limits=default_limits()){
    mfd_state_v1* next=nullptr;auto code=mfd_state_v1_new(epoch.data(),&limits,sizeof limits,&next);if(code==MFD_OK)owner_.reset(next);return code;
  }
  uint32_t admit(const uint8_t* wire,std::size_t length,page& out){
    if(length>MFD_MAX_ENVELOPE_BYTES)return MFD_LIMIT;mfd_page_v1* next=nullptr;auto code=mfd_state_v1_admit(owner_.get(),wire,static_cast<uint32_t>(length),&next);if(code==MFD_OK)out.owner_.reset(next);return code;
  }
  uint32_t admit(const std::vector<uint8_t>& wire,page& out){return admit(wire.data(),wire.size(),out);}
  uint32_t accept(const page& p){return mfd_state_v1_accept_page(owner_.get(),p.owner_.get());}
  uint32_t snapshot(mfd_state_snapshot_v1& out)const{return mfd_state_v1_snapshot(owner_.get(),&out,sizeof out);}
  uint32_t release(){return mfd_state_v1_release(owner_.get());}
};
}} // namespace morrow::fs_directory_v1
#endif
