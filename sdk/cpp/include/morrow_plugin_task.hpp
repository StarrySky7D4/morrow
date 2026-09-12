#ifndef MORROW_PLUGIN_TASK_HPP
#define MORROW_PLUGIN_TASK_HPP
#include "morrow_plugin_task.h"
#include "morrow_plugin_codec.hpp"
namespace morrow {
class task {
  mp_task* value_=nullptr; uint32_t status_=MP_CODEC_INVALID;
public:
  task()=default;
  ~task(){mp_task_free(value_);}
  task(const task&)=delete; task& operator=(const task&)=delete;
  task(task&& other) noexcept : value_(std::exchange(other.value_,nullptr)),status_(std::exchange(other.status_,MP_CODEC_INVALID)){}
  task& operator=(task&& other) noexcept {if(this!=&other){mp_task_free(value_);value_=std::exchange(other.value_,nullptr);status_=std::exchange(other.status_,MP_CODEC_INVALID);}return *this;}
  static task decode(const std::vector<uint8_t>& bytes){task t;if(bytes.size()>MP_MAX_TASK_BYTES){t.status_=MP_CODEC_LIMIT;return t;}t.status_=mp_task_decode(bytes.data(),static_cast<uint32_t>(bytes.size()),&t.value_);return t;}
  uint32_t status() const {return status_;}
  mp_task_view view() const {mp_task_view v{};if(status_!=MP_CODEC_OK||mp_task_get(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();return v;}
  mp_span command() const {mp_span v{};if(status_!=MP_CODEC_OK||mp_task_get_command(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();return v;}
  mp_content_request_v1 content() const {mp_content_request_v1 v{};if(status_!=MP_CODEC_OK||mp_task_get_content(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();return v;}
  mp_transform_view transform() const {mp_transform_view v{};if(status_!=MP_CODEC_OK||mp_task_get_transform(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();return v;}
  encoded_request output(const std::vector<uint8_t>& value) const {
    encoded_request out{status_,{}};if(status_!=MP_CODEC_OK)return out;if(value.size()>MP_MAX_TASK_VALUE_BYTES){out.status=MP_CODEC_LIMIT;return out;}
    out.bytes.resize(MP_MAX_TASK_BYTES);uint32_t length=0;out.status=mp_task_output(value_,value.data(),static_cast<uint32_t>(value.size()),out.bytes.data(),MP_MAX_TASK_BYTES,&length);out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
  encoded_request fail(uint32_t code,const std::string& message) const {
    encoded_request out{status_,{}};if(status_!=MP_CODEC_OK)return out;
    if(message.size()>MP_MAX_TASK_FAILURE_MESSAGE_BYTES){out.status=MP_CODEC_LIMIT;return out;}
    out.bytes.resize(MP_MAX_TASK_BYTES);uint32_t length=0;
    out.status=mp_task_fail(value_,code,reinterpret_cast<const uint8_t*>(message.data()),static_cast<uint32_t>(message.size()),out.bytes.data(),MP_MAX_TASK_BYTES,&length);out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
  encoded_request complete(const std::vector<uint8_t>& response) const {
    encoded_request out{status_,{}};if(status_!=MP_CODEC_OK)return out;
    if(response.size()>MP_MAX_MESSAGE_BYTES){out.status=MP_CODEC_LIMIT;return out;}
    out.bytes.resize(MP_MAX_TASK_BYTES);uint32_t length=0;
    out.status=mp_task_complete(value_,response.data(),static_cast<uint32_t>(response.size()),out.bytes.data(),MP_MAX_TASK_BYTES,&length);out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
};
}
#endif
