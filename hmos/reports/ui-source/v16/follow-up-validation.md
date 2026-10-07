# dev.16 发布后的媒体限定观察

2026-10-07，在既有 x64 模拟器上继续观察已安装 **0.1.0-hmos-dev.16 / 1000016**。包 SHA-256 `A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C`，24,857,891 字节；不是 dev.17 包。原 [dev.16 交付报告](validation.md) 保留发布时事实，不追改。

同一 `HMOS-media-20261007-A` 草稿的 MP4 `HMOS-dev14-bars.mp4`，资产 ID 为 `asset-draft-import-eaa74272b78711cf088e3953cd54830e6f3e583084dbe34eaafd4798b9ee9e77`；另一 pin 仍为原 WAV。此次未重新导入或业务发布。

| 阶段 | 限定结果 | 原始证据 |
| --- | --- | --- |
| 原生 MP4 准备 | PASS，两次暂停 0:00/0:12，无自动播放 | [prepare](device-preview-video.log) |
| 播放/暂停 | PASS，阶段 JSON 确认播放时间推进至 0:04，暂停 0:06 并保持；playing PNG 另时捕获，显示 0:03 | [playback](device-playback-video.log) |
| 全屏 | PASS，2232×1320 横屏、同一暂停 0:06 媒体；横屏 pin 依据来自先前确认，不能算新 pin 回读 | [fullscreen](device-fullscreen-video.log) |
| 全屏定位手势 | PASS，左双击 6→0、右双击 0→10、水平拖动 10→8 | [gestures](device-seek-gestures-video.log) |
| 普通进度拖动 | PASS，暂停 8→6 | [seek](device-seek-normal-video.log) |
| Back 返回 | 首驱动因本地进度文件读取错误停止；修正后 PASS，1320×2232 竖屏、同一暂停 0:08 预览仍打开，fresh 同两 pin 回读 | [初次失败](device-back-video.log)、[最终返回](device-back-video-final.log) |
| 后台生命周期 | NOT_QUALIFIED；首驱动在播放断言停止，fresh Home 命令收到 ACK，但 PNG 仍在 Morrow 应用内；不证明已进入后台或触发应用暂停 | [首次记录](device-background-video.log)、[fresh 失败](device-background-video-fresh.log) |

这些观察只覆盖该 MP4 与 dev.16 包的所列阶段；手势节点结果不证明完整反馈动画/过渡视觉资格。真实音频输出、后台/中断、其他格式、PDF/系统预览、多选/空标题、ARM64 真机和签名仍未取得本次资格。不能作为 dev.17 富剪贴板或新包回归通过。
