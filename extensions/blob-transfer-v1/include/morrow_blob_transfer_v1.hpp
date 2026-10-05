#ifndef MORROW_BLOB_TRANSFER_V1_HPP
#define MORROW_BLOB_TRANSFER_V1_HPP
#include "morrow_blob_transfer_v1.h"
#include <utility>
#include <vector>
namespace morrow::blob_transfer_v1 {
class receiver;
/* Move-only opaque owner. Views expire at owner reset/destruction; ordinary
 * allocations may throw/abort. Status errors preserve owner/vector/output. */
class frame {
 mbt_frame *p_=nullptr;
 void reset(mbt_frame *p) noexcept {mbt_frame_free(p_);p_=p;}
 friend class receiver;
public:
 frame() noexcept=default;
 ~frame(){mbt_frame_free(p_);}
 frame(const frame&)=delete;frame&operator=(const frame&)=delete;
 frame(frame&&other) noexcept:p_(std::exchange(other.p_,nullptr)){}
 frame&operator=(frame&&other) noexcept{if(this!=&other){reset(std::exchange(other.p_,nullptr));}return *this;}
 bool empty()const noexcept{return p_==nullptr;}
 uint32_t assign(const mbt_view&v){mbt_frame*p=nullptr;auto s=mbt_frame_set(&v,sizeof(v),&p);if(s==MBT_OK)reset(p);return s;}
 uint32_t decode(const uint8_t*bytes,uint32_t length){mbt_frame*p=nullptr;auto s=mbt_frame_decode(bytes,length,&p);if(s==MBT_OK)reset(p);return s;}
 uint32_t view(mbt_view&v)const{return mbt_frame_view(p_,&v,sizeof(v));}
 uint32_t encode(std::vector<uint8_t>&output)const{std::vector<uint8_t>tmp(MBT_MAX_FRAME_BYTES);uint32_t n=0;auto s=mbt_frame_encode(p_,tmp.data(),static_cast<uint32_t>(tmp.size()),&n);if(s==MBT_OK){tmp.resize(n);output.swap(tmp);}return s;}
};
class receiver {
 mbt_receiver*p_=nullptr;
 void reset(mbt_receiver*p)noexcept{mbt_receiver_free(p_);p_=p;}
public:
 receiver()noexcept=default;~receiver(){mbt_receiver_free(p_);}
 receiver(const receiver&)=delete;receiver&operator=(const receiver&)=delete;
 receiver(receiver&&other)noexcept:p_(std::exchange(other.p_,nullptr)){}
 receiver&operator=(receiver&&other)noexcept{if(this!=&other)reset(std::exchange(other.p_,nullptr));return *this;}
 bool empty()const noexcept{return p_==nullptr;}
 uint32_t create(){mbt_receiver*p=nullptr;auto s=mbt_receiver_new(nullptr,0,&p);if(s==MBT_OK)reset(p);return s;}
 uint32_t create(const mbt_limits&limits){mbt_receiver*p=nullptr;auto s=mbt_receiver_new(&limits,sizeof(limits),&p);if(s==MBT_OK)reset(p);return s;}
 uint32_t accept(const uint8_t*wire,uint32_t length,uint32_t&kind,frame&receipt){uint32_t k=0;mbt_frame*p=nullptr;auto s=mbt_receiver_accept(p_,wire,length,&k,&p);if(s==MBT_OK){kind=k;receipt.reset(p);}return s;}
 uint32_t snapshot(mbt_snapshot&out)const{return mbt_receiver_snapshot(p_,&out,sizeof(out));}
 uint32_t cancel(){return mbt_receiver_cancel(p_);}
};
}
#endif
