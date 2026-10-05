#ifndef MORROW_FS_DIRECTORY_REQUEST_V1_H
#define MORROW_FS_DIRECTORY_REQUEST_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MDR_VERSION 1u
#define MDR_MAX_REQUEST_BYTES 512u
#define MDR_MAX_RESPONSE_BYTES 65536u
#define MDR_MAX_PAGE_BYTES 65536u
#define MDR_FEATURE "fs-directory-request-v1"
#define MDR_IMPORT_MODULE "morrow_fs_directory_v1"
#define MDR_IMPORT_NAME "call"
enum { MDR_OK=0,MDR_INVALID=1,MDR_CONTRACT=2,MDR_LIMIT=3,MDR_BUFFER=4,MDR_CORRELATION=5,MDR_STATE=6,MDR_BUDGET=7,MDR_UNSUPPORTED=8,MDR_OUTCOME_UNKNOWN=9 };
enum { MDR_OPEN=0,MDR_NEXT=1,MDR_FINISH=2,MDR_CANCEL=3 };
enum { MDR_REPLY_OPENED=0,MDR_REPLY_PAGE=1,MDR_REPLY_FINISHED=2,MDR_REPLY_CANCELLED=3,MDR_REPLY_ERROR=4 };
/* Wire error status values are distinct from C function status values. */
enum { MDR_STATUS_DENIED=1,MDR_STATUS_CLOSED=2,MDR_STATUS_BUSY=3,MDR_STATUS_INVALID=4,MDR_STATUS_LIMIT=5,MDR_STATUS_UNSUPPORTED=6,MDR_STATUS_OUTCOME_UNKNOWN=7 };
enum { MDR_UNOPENED=0,MDR_READY=1,MDR_PENDING=2,MDR_TERMINAL=3,MDR_UNKNOWN=4 };
typedef struct mdr_request mdr_request;
typedef struct mdr_response mdr_response;
typedef struct mdr_client mdr_client;
typedef struct mdr_request_view {
 uint32_t abi_version,struct_size,action,has_after_entry_id;
 uint8_t nomination_ref[32],nonce[32],selection_epoch[32],after_entry_id[32];
 uint64_t page_sequence;
 uint32_t reserved[2];
} mdr_request_view;
typedef struct mdr_reply_view {
 uint32_t abi_version,struct_size,kind,status;
 uint8_t selection_epoch[32];
 uint64_t page_sequence,metadata_bytes;
 uint32_t entries,reserved;
 const uint8_t *page_wire;
 uint32_t page_wire_length,reserved2;
} mdr_reply_view;
typedef struct mdr_response_view {
 uint32_t abi_version,struct_size,action,reserved;
 uint8_t nomination_ref[32],nonce[32],request_digest[32];
 mdr_reply_view reply;
} mdr_response_view;
typedef struct mdr_snapshot {
 uint32_t abi_version,struct_size,phase,reserved;
 uint64_t calls,request_wire_bytes,response_wire_bytes,next_sequence,resident_nonces;
} mdr_snapshot;
typedef int32_t (*mdr_transport_fn)(void *context,const uint8_t *input,uint32_t length,uint8_t *output,uint32_t capacity);
/* Handles own all decoded bytes; views borrow a live owner until free/replace.
 * Native callers supply readable/writable valid aligned storage, live exclusive
 * handles, and no forged/stale pointer/double free. This is not pointer isolation.
 * Struct inputs require abi_version1, exact struct_size and all unused/reserved
 * fields zero. Output structs require size>=sizeof. NULL bytes only for length0;
 * requests/pages need nonempty validated wire. Ordinary allocator OOM is not
 * promised recoverable. Successful slots replace old values without freeing them.
 * Error returns preserve output slots/buffers/length; admitted client fees remain.
 * encode/view output must not overlap any handle or its owned buffers; prefix
 * and length outputs cannot overlap. client input/output must not alias any
 * client-owned allocation. decode/set may alias byte/view input and output slot
 * when no other live handle storage is affected: input is owned before write.
 * Response decode/set slots cannot alias the original request/owned wire.
 * Client counters are helper costs, never original IoBinding authority; no now
 * or host-approved bool is accepted. Cancelled/Finished do not prove nativejoin.
 * Callback is a single synchronous transport operation, not a grant/provider
 * authority. It receives capacity exactly65536 and cannot retain borrowed buffers.
 * Nonpositive/oversized callback length is OutcomeUnknown; no second call/retry.
 * Without callback fixed wasm import is used; native call returns Unsupported.
 * Successful decoding never authorizes picker/public FileList; no reentrant client
 * calls or access to the same handle during a transport callback are permitted.
 */
uint32_t mdr_request_set(const mdr_request_view*,uint32_t,mdr_request**);
uint32_t mdr_request_decode(const uint8_t*,uint32_t,mdr_request**);
void mdr_request_free(mdr_request*);
uint32_t mdr_request_get_view(const mdr_request*,mdr_request_view*,uint32_t);
uint32_t mdr_request_encode(const mdr_request*,uint8_t*,uint32_t,uint32_t*);
uint32_t mdr_response_set(const mdr_request*,const mdr_reply_view*,uint32_t,mdr_response**);
uint32_t mdr_response_decode_for(const uint8_t*,uint32_t,const mdr_request*,mdr_response**);
void mdr_response_free(mdr_response*);
uint32_t mdr_response_get_view(const mdr_response*,mdr_response_view*,uint32_t);
uint32_t mdr_response_encode(const mdr_response*,uint8_t*,uint32_t,uint32_t*);
uint32_t mdr_schema_digest(uint8_t*,uint32_t);
uint32_t mdr_page_schema_digest(uint8_t*,uint32_t);
uint32_t mdr_client_new(const uint8_t *nomination_ref,uint32_t length,mdr_client**);
void mdr_client_free(mdr_client*);
uint32_t mdr_client_snapshot(const mdr_client*,mdr_snapshot*,uint32_t);
uint32_t mdr_client_begin(mdr_client*,const mdr_request_view*,uint32_t,mdr_request**);
uint32_t mdr_client_begin_request(mdr_client*,const mdr_request*,mdr_request**);
uint32_t mdr_client_accept(mdr_client*,const uint8_t*,uint32_t,mdr_response**);
uint32_t mdr_client_transport_unknown(mdr_client*);
uint32_t mdr_client_call_once(mdr_client*,const mdr_request_view*,uint32_t,mdr_transport_fn,void*,mdr_response**);
uint32_t mdr_client_call_request_once(mdr_client*,const mdr_request*,mdr_transport_fn,void*,mdr_response**);
#ifdef __cplusplus
}
#endif
#endif
