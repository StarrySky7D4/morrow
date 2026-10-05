# C05 channel payload 独立分发资格

2026-10-05。本地新source-only profile保留sdk/与两extensions/的Cargo路径，90源码文件＋manifest。原SDK库只选62文件，不增改SDK327／冻结57；旧分发tool/profile和原 sdk.lock.toml不变。独立验证器不包含Core/runtime/host或compiler，接口见 [接入指南](../../docs/PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md)。

## 校验与构建

最终27个Python方法通过、零skip；实际目录／ZIP／重定位CLI执行通过。覆盖sha篡改、缺失／额外文件或空目录、重复／大小写、JSON重复／bool版本、边界、路径／设备名、ZIP链接／reparse属性／CRC／compression、无覆写及发布竞争、source drift、原锁与依赖边、TOML bool/int区分及非string checksum。真实无特权Windows junction被拒绝且target保持，没有开启Developer Mode或修改安全设置。

首次只读审核发现三个锁协同改动仍可通过、path包依赖边未校验及编译说明遗漏；原发现和内存攻例保留。固定原SDK lock身份、逐项依赖关系和三语言说明后审核通过；后续补严格TOML类型和checksum拒绝。27是最终原始日志的逐名方法数。旧驱动只匹配单行结果，重复ZIP名称测试的警告把ok移至后续行，导致旧汇总少记为26；旧日志和回执保留。早期汇总不是额外方法，不累计。

在项目仓库外、只含独立源码闭包的重定位目录真实编译：两native Release库、两个Rust wasm32-unknown-unknown guest、两个c-transport静态archive、C支持源／C++runtime以及四个Clang WASI C11／C++17 guest。30条实际命令全部exit0（含版本／验证命令，不是30个测试方法）；构建输入只来自独立源码／toolchain／新离线公共cache／sysroot，没有R源码、headers或Core。13公共依赖单独按原hash复制，避免Cargo修改旧cache记账。

旧分发版本的构建90源码保持封存；最终只收紧其验证器，另导出、解包和验证新的90文件，逐项对照其余89与全部编译输入完全同字节。库／headers／guest／Cargo／锁没改，不将两个manifest称作完全相同，也不重复编译来增加资格计数。

构建回执 `965162adf20c0adde83b31c703729b8e94dc4470fe569ae95cc6a47623e48db9`；最终source ZIP SHA256 `388dcefffd054a6b07719b09fe7712fe8af927aadb84bf91c64d3b4831c3bc8f`，182,156 bytes。ZIP是本地源码分发，不是GitHub Release、签名或全SDK冻结。

## 新产物的执行

六新Wasm都与仓库内构建的对应旧示例binary不同，保留section／首差异和双方hash，不推测原因、不继承旧guest执行结果。用已钉住的可信原host工具分别包装六个NEW包，原package没有重封装；再真实执行WS2／SSE2方法，六语言成功组合＋十二撤权／Stop组合均由raw记录证明。

四方法全通过、零filter/skip；检查完整字段、原wire／ACK／cursor／transcript、Unknown、单连接／POST和各层实际join。这四个方法名属于既有网络100，属于新产物的重复验证，不记104。包包装与runtime共8条命令；不混为8个业务测试。

运行回执 `02781b1f04d46ba5907c8d94131e8d21ca42bab9dd587896d3bb0766ab59e7dd`。每条实际命令有工具／binary／模块／包／源码前后pin和新合成TEMP，旧native target只读，未重建旧原件。

## 保留限制

第一次root导出驱动在运行unit之前引用不存在的Core文件，第一次外部build驱动在编译之前找错历史receipt名字；各次runner/范围与失败保留，并在新目录修正。它们不是SDK代码失败，也不记为编译／方法通过。

清单是unsigned byte integrity，不证明任意源码语义、恶意同用户并发路径安全、macro展开或普通构建的sandbox。实际宿主包装依赖可信仓库tools，独立source-only CLI尚未提供host安装审批。生产owner/GUI、普通token、真实账户／TLS/服务、其他平台、发布与完整SDK仍OPEN。
