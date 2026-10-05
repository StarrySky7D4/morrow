# 2026-10-05 dev.11 自定义卡片排序来源审计

本次核对实际工作树 HEAD、Flutter 应用版本、相关源码 Git blob 和原始字节 SHA-256；没有运行 Windows 测试、操作设备或修改参照工作树。以下为观察值，原功能跟进仍在推进。

| 活跃工作树 | HEAD | `pubspec.yaml` |
|---|---|---|
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` |
| `build/win-cloud-20261005` | `772466177fe589cee53bc633e69f411c34610104` | `0.1.9-test.58+62` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` |

三个工作树的 `lib/main.dart` Git blob 同为 `2e00f41251c3215f0b6d886712c86adfdf2bb6e6`；相关排序源码和 pubspec 本次未发现已跟踪修改。排序参照的 Git blob 也分别完全一致：

| 源码 | Git blob |
|---|---|
| `lib/card_order_preferences.dart` | `49d0706dbef1e48bc791e049a86899c6eae958a3` |
| `lib/pages/card_order.dart` | `c63c46511532d4bc0fc8d803de6dcaadeacfcde1` |
| `lib/hold_reorder.dart` | `2ed1ab3b4b26bad3a3ba4c49050ef6b76980898e` |

`io-safety-refactor` 原始字节 SHA-256 分别为：preferences `34AAAB493EB2E05A4B0BAA76921EDA47484649DC11638FE8B0D28C86368C1D73`；pages/card_order `6A1A9C29D2345C8111EE213534A40EA3C1841700EA8A57FFCBA788B4CB4F4FB4`；hold_reorder `6776A3B8B45FF18F7D4C04F29182E7B77A6675E9B8F4A38AAFF5C4F415478062`。HMOS 是逻辑移植，不宣称 ArkTS 文件字节相同，也未覆盖原冻结来源清单。

原规则为：使用五个稳定 page ID；按页持久化 `{version:1,orders:{page:[id...]},manual:[page...]}`。普通 recent、favorites-first、title 模式仍由 Studio 管理，不是每页独立的持久化排序枚举。`order` 只投影当前 ID 集合，已保存 ID 优先、新 ID 追加；读取不永久清理缺失 ID。`move` 按目标身份插入，将移动后的 visible 数列填回完整数列中的 visible 槽，筛选隐藏卡片位置不动，并在该次写入中清理已移除身份。`_save` 先展示候选，保存失败恢复完整旧 orders/manual，没有自动重试。

HMOS `model/CardOrder.ets` 保留上述语义，注入只读偏好读取和偏好写入，不发送业务数据库命令。适配当前 256 张卡片上限，将源每页 10000 的预算收紧为 256；仅接受五个稳定页面，回收站不参与自定义排序。恢复内容先完整校验后原子替换，非法版本、重复 ID、超限和非法页面保留原状态。

拖动来源参照 `hold_reorder.dart` 的 scope/revision 判断与 `moveRelative` 身份移动。模型增加 `setView` 的完整 page/all/visible/token 快照和 expectedRevision/expectedViewToken 门；Index 需把 page、filter、query、sort、content generation、query phase/IDs 都纳入 token。偏好异步回执只更新对应页面的偏好，普通 sort 的失败恢复仍须像 Flutter 一样检查当前页没有变化。

`hmos/tool/card-order-model.test.cjs` 执行实际 ArkTS 模型，本轮 19 项通过，覆盖 hidden anchors、查询顺序不同、新增/删除、持久化重启、完整回滚、非法恢复原子性、过期视图、切页、dispose 和完整 256 张候选。该证据不等于原生拖动手势、设备 UI、Windows 测试或生产存储资格。完整 HTML/附件、captured S1/S2 和生产身份/密钥/封存边界保持原状态。

dev.11 接线只读审阅：Index 先按当前 pageCards 身份集合投影 query IDs，分类、收藏、删除回执后尚未更新的查询 ID 不会进入不相容页面；all/visible 仍分别保持完整页面成员与当前筛选结果，隐藏槽规则可正确应用。查询未 ready 或仍 refreshing、偏好读写未结束时拒绝移动；完整 token、revision、当前 CardView.source 和拖动 nonce 一起拒绝旧视图/外部拖动。回收站清除自定义排序 view，不写入 manual 页面。

本轮审阅发现一个界面回滚边角：回收站普通排序当时绕过偏好 busy/loading 门，可能在 A 页模式写入中改变全局 sort，再返回 A 后被旧失败回执恢复为更早的 sort。修正建议是统一选择门或按 sort 选择 epoch 核对回滚；该问题在接线侧，模型没有发现对应缺陷。此处只记录源码审阅，设备拖动落点、虚拟化和边缘滚动表现仍需实际验证。
