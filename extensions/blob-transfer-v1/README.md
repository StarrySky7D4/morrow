# blob-transfer-v1

独立实验 Rust/C/C++17 分段字节 codec 与同步 `Receiver`，version 1，raw schema SHA256 `941db7c662815f8963b46bbb65a5c143d90f9e7217937e84c05d1b8f11024543`。Windows Release 库内 23 个唯一方法通过：codec 6、state 10、FFI 7；strict lint与自身格式检查通过。Debug只输出类型/长度/序号/offset/阶段，不输出正文、opaque IDs或digest数组；owned/borrowed及嵌套状态均有回归。blob62个wire vectors（15接受/47拒绝）的Windows native三语言消费者与独立状态检查通过；没有新增Wasm或guest协商，G04/SDK26 仍 OPEN。

`Frame`/`FrameRef` 表示 Descriptor、Chunk、Receipt 或 End，固定 nonzero transfer_epoch/object_ref/operation_id。Descriptor 固定 total_length/whole_sha256；Chunk 使用连续 sequence/offset、完整 payload 和 chunk_sha256；Receipt 绑定原 wire request_digest 和全部 chunk 字段，状态 Accepted/Existing；End 重申原总长度和整体摘要。对象上限 16 MiB、chunk 60 KiB、wire 65,536 bytes，有限 traversal 32,768 words/nesting 16，无尾随消息或 capability。

owned 分配前检查字段/range/digest；默认 allocator 保持不变。borrowed decode 无 allocator 解析 segments，并把 payload 映射为调用者原 wire 内的 checked slice；生命周期属于原 wire，不借用临时 owned segment。固定 capnp 0.24.1 显式启用 unaligned，避免原生未对齐引用。owned frame 可在输入释放后继续使用。

`Receiver` 固定 Descriptor/ID，实时 streaming SHA256 验证唯一连续 payload，而非相信输入 digest。同步调用无 pending 队列，至多保留最近一个 <=64 KiB 原请求，不存整个对象。只有同 live 状态最近原 wire 逐字节及摘要/字段一致时返回 Existing；不同编码的同语义或旧请求不冒充已接受。gap、overlap、foreign ID、错误 chunk/whole hash、提前 End 不推进接受状态。

helper 分开累计 payload、request/response wire、总 wire 与 request/receipt 数；duplicate 仍收费但不重复 payload/rehash。已 admitted 费用在错误或 cancel 后保留，清理不退费。默认 payload/request/response/总 wire 为 16/32/4/36 MiB，request/receipt 各 1,024；可收紧，独立于真实 IoBinding 的 authoritative 预算。cancel 释放最近请求，verified/cancelled 状态不再接受。

End 只在实际长度和 streaming whole hash 都匹配时返回 `VerifiedBytes`；空对象验证 SHA256(empty)。这不是 Store 持久化、FileCreate、上传或业务 commit，也不是 durable channel ACK。没有 source/lease/grant、clock 或重新授权参数。

C 的 `include/morrow_blob_transfer_v1.h` 提供 opaque frame/receiver owners、checked view、snapshot、accept 与 cancel。views 借用 owner，原生指针有效性、尺寸、对齐、alias 和独占访问遵循头文件；输入/output slot 可按约定重叠，receiver 原 wire 不能和 receiver/retained storage 重叠。错误保持输出，已 admitted helper 费用例外。C++17 `include/morrow_blob_transfer_v1.hpp` 提供 move-only owners、失败保持的 vector 编码。普通 OOM 不承诺恢复，指针检查不是沙箱。

库输出 rlib/staticlib，依赖版本限定原 SDK 注册依赖集合，不新增 required_feature/import 或权限。真实 backend、upload、watch、rename、owner 接线仍 OPEN；conditional Replace Unsupported，不能退化为 overwrite。外部结果 Unknown 不凭 receipt/VerifiedBytes 自动重放。详见 [接口](../../docs/PLUGIN_DIRECTORY_BLOB_SDK.md)、[本轮记录](../../reports/reconstruction-2026-10-05/directory-blob-sdk.md)。
