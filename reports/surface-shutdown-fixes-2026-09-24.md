# 组件分离与内容服务关闭 · 2026-09-24

本轮完成用户请求的风格组件粘连修复与退出交互优化。应用维持 0.1.9-test.54+58，内置插件维持 test.54.6；未提交、推送或发布。

## 组件绘制

- Glass、风格控件与卡片交互层具有 6px 外绘制预算，避免深度阴影进入邻居组件；不增加布局尺寸，不替换控件身份。
- 新拟态、黏土、Fluent 缩小投影位移和模糊范围，粗野主义高深度侧影上限从 8px 收至 5px。渲染检查发现单纯裁边会产生硬块，因此同步调整阴影参数后重新审图。
- 卡片悬停不再按卡片宽度向外放大；保留上浮、轻量投影与按压。主界面紧密按钮组统一增加到 12px 间隔，风格候选项间隔增加到 14px。
- 像素回归覆盖七种风格、三种玻璃、明暗主题、200% 深度：14px 间距中保留 2px 无阴影通道。风格切换时文本控件 State、内容不丢失。

## 关闭流程

- 专用关闭页替代恢复重试页，提供“转入后台继续关闭”；在关闭页再次按窗口 X 也可转入后台。
- 后台不释放内容库所有权，不允许另起写入者，不以超时冒充退出。真实进程退出才销毁窗口；失败时或后台等待 30 秒仍未完成时重新显示。
- hide/show/destroy 串行化，覆盖 hide 尚未完成时遇到失败/真实退出的竞态；窗口销毁失败允许再次关闭重试。
- 启动阶段收到关闭请求后，不再用工作台或恢复页覆盖关闭页；新增九语言关闭文案。
- Rust 撤销监听许可、发出 stop 后，同时等待监听器及 worker/Store 完成，避免这两段收尾串行叠加；即使监听器失败也必须等待 worker 的真实结束。

## 验证

- 47 项不同 Dart 测试通过：风格与交互 34 项、明暗控件视觉基准 2 项、退出/所有权/真实子进程共 11 项。
- Rust 服务 28 项、CLI EOF 3 项通过。确定性并行测试证明两路同时开始、监听器失败不会提前释放 owner。
- 12 个目标静态分析无问题；九语言生成一致性与目录测试由 UI 子代理验证通过。
- Windows Release INSTALL 构建通过，目标 lib/main_rust.dart。产物目录：build/windows-corners/x64/runner/Release，入口 morrow_studio.exe（需要同目录 DLL、data、plugins）。
- 已人工查看相邻风格预览、明暗控件预览；视觉基准更新后再次比对通过。

日志：build/surface-separation-regression.log、surface-separation-final.log、shutdown-native-regression.log、shutdown-ui-final.log、surface-shutdown-rust-service.log、surface-shutdown-rust-cli.log、surface-shutdown-analyze.log、surface-shutdown-windows.log。

## 验证边界

窗口转入后台并不等于内容服务已退出。EOF 前已接受的串行请求仍可能等到自身执行/超时，长时间同步回调仍可能延迟服务结束；本轮未强杀服务，也未实测产品级退出耗时下降。真实子进程测试使用受控 Python 子进程，Rust 测试覆盖实际服务及 CLI EOF；Windows 仅构建，未进行实机点击退出/GPU Profile。正式 UI autosave 的原始值/附件完整工作台所有权仍是已有开放项，本轮没有宣称补完。
