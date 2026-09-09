# Android 构建

已添加 Android 工程及移动端文件适配。最低 Android 7.0（API 24），编译／目标 API 36，Flutter 3.44.0。当前尚未完成 APK 编译与真机验证。

## 本机准备

安装 JDK 17，以及 Android SDK 的 Command-line Tools、Platform-Tools、Platform 36、Build-Tools 36.0.0、NDK 28.2.13676358。SDK Manager 提示的许可证需在本机阅读并接受。无需为构建安装模拟器。

官方说明：[Flutter Android 环境](https://docs.flutter.dev/platform-integration/android/setup)、[Android SDK 工具](https://developer.android.com/tools/sdkmanager)。

项目内安装入口（复用 `tool/vendor/java/` 中的 JDK；SDK 默认保存在 Git 忽略的 `tool/vendor/android-sdk/`）：

```powershell
pwsh -File tool/setup_android.ps1
# 官方下载无法直连时，使用本机可用的 HTTP 代理：
pwsh -File tool/setup_android.ps1 -Proxy http://127.0.0.1:10809
```

安装脚本校验官方 Command-line Tools 下载包的 SHA-256，安装上述 SDK/NDK 与 CMake 3.22.1；许可证由 SDK Manager 在终端显示并交互确认。安装成功后才保存 Flutter 的 SDK/JDK 路径。指定的代理仅用于 SDK 下载与 SDK Manager；Gradle/Maven 构建仍需要可用网络。

已有 SDK 时，可在当前 PowerShell 会话加载路径：

```powershell
. ./tool/enter_android.ps1 -AndroidSdk 'C:\你的AndroidSDK目录'
# 使用项目内默认 SDK 时：
. ./tool/enter_android.ps1
```

不设置系统级环境变量；新终端重新加载脚本即可使用 `java`、`adb`、`sdkmanager`。

```powershell
flutter doctor --android-licenses
flutter doctor -v
flutter pub get
flutter analyze
flutter test
pwsh -File tool/build_android.ps1 -AndroidSdk 'C:\Users\你的用户名\AppData\Local\Android\Sdk' -Jdk 'C:\你的JDK17目录'
```

脚本默认构建 ARM64 调试预览 APK，保存至 `dist/morrow-<版本>-android-arm64-debug.apk`，附 SHA-256。已设置环境变量时可省略路径；也会识别项目忽略目录 `tool/vendor/java/` 内的 JDK。32 位手机用 `-Architecture android-arm`，x64 模拟器用 `-Architecture android-x64`。

也可直接运行 `flutter build apk --debug --target-platform android-arm64`，产物在 `build/app/outputs/flutter-apk/app-debug.apk`；连接设备后可用 `flutter run -d <设备ID>` 或 `adb install -r <APK路径>`。

## 签名

预览包使用本机调试签名，包名 `dev.starrysky7d4.daemon.preview`。正式包名为 `dev.starrysky7d4.daemon`，两者可并存，数据独立。预览包不用于正式分发；换电脑后调试签名可能变化。

正式构建需自己的长期签名密钥及 `android/key.properties`，例如：

```properties
storeFile=C:/private/daemon-release.jks
storePassword=你的存储密码
keyAlias=daemon
keyPassword=你的密钥密码
```

配置后运行脚本的 `-Release` 或 `flutter build apk --release`。未配置时明确停止，避免把调试签名当作正式签名。密码文件、JKS／keystore 已排除出 Git，密钥需自行备份，后续覆盖安装使用相同签名。

## 手机端行为与验证范围

- 复用单栏／平板布局；内容避开系统状态栏、导航栏和键盘，状态栏图标跟随深浅主题。手机不显示 Windows 窗体圆角控制。
- Android 的透明选项保留透明面板，以当前主题作为应用承载底色，不透出桌面或其他应用。
- 系统选择器导入文件；附件副本保存在应用私有目录。另存使用系统“创建文档”；外部打开使用只读、临时授权的 `content://` URI。无需申请全盘存储权限。卸载／清除应用数据会删除内部内容，重要附件请先另存。
- 歌词按外部文件、内置标签、联网搜索读取。Android 不保证能读取歌曲原目录的相邻文件，请同时选择歌曲与同名 `.lrc`，或手动导入歌词。歌词选择器显示所有文件，以兼容没有 LRC MIME 类型的文件管理器；选择后校验扩展名。
- Android 剪贴板文件取决于来源应用提供的格式；不可读取时用“导入文件”。CAD／Blender 作为附件保存，外部打开需要手机安装对应应用。
- 已声明联网权限；网络媒体优先使用 HTTPS。未添加后台播放服务、锁屏控制或音频焦点管理，这些不属于本轮准备内容。
- 原生文件选择、取消另存、CAD 外部打开、音视频解码、剪贴板 content URI、旋转与系统返回仍需在真实 Android 设备验证。组件测试不能代替这些验证。

本轮检查：`flutter analyze` 无问题，33 项单元／组件测试全部通过，包括新增 Android 系统边距、键盘与状态栏图标测试；PowerShell 构建脚本语法检查通过。

JDK 17 已在项目忽略目录就绪；Google SDK 下载端点连接超时，实际 `flutter build apk --debug` 停在 `No Android SDK found`，因此没有可安装 APK，原生 Kotlin／Gradle 尚未完成编译验证。

2026-09-09 环境复查：Flutter 3.44.0 / Dart 3.12.0、Microsoft OpenJDK 17.0.20.1 已确认；Android SDK 未安装，未发现 Android 设备。`flutter doctor -v` 的 Android toolchain 未通过，Google Maven 网络检查超时。Google 官方主下载地址及两个备用地址均超时。新增安装／会话加载脚本，并让构建脚本自动识别项目内 SDK；这不代表 SDK 安装或 APK 编译已经通过。
