# Android 构建

2026-09-15 补充：本文保留旧 Android 构建与设备记录，不表示新 Rust 插件架构已经接入。新架构的隔离 Service、Cap’n Proto 传输、共享对象、后台恢复及 API 分级任务见 [Android 插件执行域方案](ANDROID_PLUGIN_RUNTIME.md)。实际最低系统版本须按构建配置和能力分别核对。

已添加 Android 工程及移动端文件适配。最低 Android 7.0（API 24），编译／目标 API 36，Flutter 3.44.0。ARM64 调试 APK 已完成构建与包内容检查，已在 Android 16 真机安装启动并连接 Flutter 调试器，完整功能验证尚未完成。

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

脚本默认按 ABI 拆分，构建 ARM64 调试预览 APK，保存至 `dist/morrow-<版本>-android-arm64-debug.apk`，附 SHA-256。已设置环境变量时可省略路径；也会识别项目忽略目录 `tool/vendor/java/` 内的 JDK。32 位手机用 `-Architecture android-arm`，x64 模拟器用 `-Architecture android-x64`。

也可直接运行 `flutter build apk --debug --target-platform android-arm64 --split-per-abi`，产物在 `build/app/outputs/flutter-apk/app-arm64-v8a-debug.apk`；连接设备后可用 `flutter run -d <设备ID>` 或 `adb install -r <APK路径>`。

## 按需使用 Maven 镜像

`tool/android_mirrors.init.gradle` 将 Google Maven、Maven Central 和 Gradle Plugin Portal 映射到阿里云对应仓库，同时保留其他自定义仓库。仅在本次 Gradle 命令显式指定时生效，不修改全局 Gradle 配置。仓库地址参考[阿里云官方说明](https://help.aliyun.com/zh/document_detail/436767.html)。

在项目根目录执行以下 ARM64 调试构建；先运行 `flutter pub get` 更新插件配置：

```powershell
. ./tool/enter_android.ps1 -AndroidSdk "$env:LOCALAPPDATA/Android/sdk"
flutter pub get
$previousOptions = $env:JAVA_TOOL_OPTIONS
try {
    $socketDir = New-Item -ItemType Directory -Force build/android-env-check
    if ($env:JAVA_TOOL_OPTIONS -notmatch 'jdk\.net\.unixdomain\.tmpdir=') {
        $env:JAVA_TOOL_OPTIONS = ($env:JAVA_TOOL_OPTIONS + ' -Djdk.net.unixdomain.tmpdir="' + $socketDir.FullName + '"').Trim()
    }
    ./android/gradlew.bat -p android --init-script "$PWD/tool/android_mirrors.init.gradle" '-Ptarget-platform=android-arm64' '-Psplit-per-abi=true' '-Ptarget=lib/main.dart' '-Pbase-application-name=android.app.Application' '-Pdart-obfuscation=false' '-Ptrack-widget-creation=true' '-Ptree-shake-icons=false' :app:assembleDebug
    if ($LASTEXITCODE -ne 0) { throw "Gradle build failed ($LASTEXITCODE)." }
} finally {
    $env:JAVA_TOOL_OPTIONS = $previousOptions
}
```

产物位于 `build/app/outputs/flutter-apk/app-arm64-v8a-debug.apk`。这个入口直接调用 Gradle；应用版本读取 `android/local.properties` 的 `flutter.versionName` / `flutter.versionCode`，修改 `pubspec.yaml` 版本后需同步这两个值。常规 `tool/build_android.ps1` 仍通过 Flutter 构建并自动维护版本。

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

## 2026-09-09 已完成的环境配置

- Flutter 3.44.0、Dart 3.12.0、Microsoft OpenJDK 17.0.20.1；Flutter 已保存 Android Studio、Java 和 SDK 路径。
- SDK 位于 `C:\Users\Administrator\AppData\Local\Android\sdk`。已补齐 Command-line Tools 22.0、Platform API 36（revision 2）以及插件需要的 API 31、34、35、Build-Tools 36.0.0、NDK 28.2.13676358、CMake 3.22.1；保留原有 API 37 与模拟器组件。
- 全部 SDK 许可证已接受，`flutter doctor -v` 返回 `No issues found!`，网络检查通过。
- Gradle 9.1.0 已下载，并通过 JDK 17 启动检查。
- 原生 Java Selector 在默认临时路径上出现 `Unable to establish loopback connection` / `UnixDomainSockets.connect0: Invalid argument`。指定项目内的 socket 临时目录后，最小 Selector 测试和 Gradle 启动均通过。构建脚本通过 `JAVA_TOOL_OPTIONS` 传递此设置，保留已有显式设置，并在退出时恢复原值；未修改系统设置。参数说明见 [Java Networking](https://docs.oracle.com/en/java/javase/17/core/java-networking.html)。
- 当前没有连接的 Android 真机或运行中的模拟器。环境检查不替代 APK 编译和设备运行验证。

```powershell
pwsh -File tool/build_android.ps1
```

## 2026-09-09 兼容性修复与 APK 验证

`android/cargokit_gradle9_compat.gradle` 由根 Android 构建脚本加载，仅为 `irondash_engine_context` 和 `super_native_extensions` 的 Gradle 9 构建提供 `project.exec` 到 `ExecOperations` 的兼容适配。旧 API 的移除说明见 [Gradle 9 升级文档](https://docs.gradle.org/current/userguide/upgrading_major_version_9.html)。插件更新为原生支持新 API 后可移除此适配层。

适配保留子进程工作目录、参数、环境和非零退出失败语义，从 `android/local.properties` 补齐 `FLUTTER_ROOT`，创建 Windows 启动脚本提前写入的 `.dart_tool` 目录，并在任务结束时检查所有所需原生库存在且非空。未修改 Pub 缓存中的插件源码。

- 阿里云 Maven 镜像已实际拉取 AGP、AndroidX 与 Kotlin 依赖；个别 AndroidX 文件使用华为云补拉，并与 Google 官方 SHA-1 核对。Flutter 调试引擎由 Google Storage 下载完成。
- Cargokit 的 GitHub 普通下载入口连接中断后，改用官方 Release Asset API 获取两个插件的 ARM64、x86、x64 预编译库及签名。六个库均通过插件内置公钥的 Ed25519 验证后才写入缓存。下载与校验材料保存在 Git 忽略目录 `build/android-env-check/native-download/`。
- 隔离 Gradle 检查通过：环境变量、工作目录、非零子进程退出传播、适配作用域，以及缺库时拒绝继续打包。日志为 `build/android-env-check/compat-smoke-exec.log` 和 `compat-smoke-missing.log`。
- ARM64 APK 构建成功：最后一次耗时 22 秒，292 个任务（33 执行，259 使用已有结果）。日志为 `build/android-env-check/compat-build.log`。
- APK：`dist/morrow-0.1.7-android-arm64-debug.apk`，107,005,987 字节。包名 `dev.starrysky7d4.daemon.preview`，版本名 `0.1.7-preview`，版本码 `2008`（基础版本码 8 加 Flutter 的 ARM64 拆分偏移），最低 API 24，目标 API 36。
- `apksigner verify --verbose` 验证通过，使用本机调试签名和 APK v2 签名。包中 8 个原生库全部验证为 ARM64 ELF，包含 Flutter、irondash、super_native_extensions、媒体和 Dart JNI 库。
- SHA-256：`14037ab0027521bf401a30062c7c989f705e2f39b1c3e588dcb56e1c70417d41`，同目录保存 `.apk.sha256` 文件。
- 最终 `flutter doctor -v` 的 Android 工具链通过、全部许可证已接受；网络检查仍报告 `https://maven.google.com/` 直连超时。上述镜像构建已成功。日志为 `build/android-env-check/flutter-doctor-final.log`。

2026-09-09 已在 vivo V2426A（Android 16、ARM64）成功安装并冷启动预览 APK，连接 Flutter attach 与 DevTools。已检查真机界面渲染及输入弹窗，捕获的应用日志未发现 Flutter 异常或致命崩溃；日志位于 `build/android-env-check/device-debug.log`。音视频、剪贴板与文件选择等完整设备功能仍待验证。正式签名构建仍需配置自己的长期密钥。
