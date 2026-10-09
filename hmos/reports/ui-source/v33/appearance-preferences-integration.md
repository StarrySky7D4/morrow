# v33 外观偏好独立模型与接入合同

当前追加资格：模型 `appearance-preferences-a2-result.json` 61/61 PASS，实际 Index `index-appearance-a1-result.json` 32/32 PASS。Index 已真实接入相同 namespace 的进程共享在途屏障与显式恢复；当前源码和范围见 `index-appearance-validation.md`。下面的初版 56 项冻结记录保留为历史 a1，不能替代当前 a2 或 SDK/设备资格。

当前基线 HEAD：`ada95e0f119b3d3c3273b8e5bf6da031d3b10c10`。本子任务只新增 `AppearancePreferences.ets` 和所属测试、报告；未修改 Index、Appearance、Rust、业务存储，未执行 SDK 构建、设备操作或 Git 发布。

## 已实现

- 状态为 `not-loaded / loading / ready / read-failed / write-unknown`。读取失败、坏 JSON、字段类型错误、缺少核心字段均禁止普通写入。不得用默认值覆盖不可读原件。
- `view().current` 是已确认完整快照；`candidate` 是最新完整 UI 候选。`originalRaw` 是最后确认原字节；`observedRaw` 保留最新已拥有的实际读取值；`unknownRaw` 固定实际已发出而结果未确认的完整候选字节。界面预览不能成为保存成功证明。
- 逐项串行：保存先重读并比较原字节，随后 `put`、`flush`、精确重读确认全部成功才更新 current。每项使用前一项已确认 original；连续滑动不会让旧 ACK 取代最新 candidate。
- 任一 put/flush/readback 失败或已发出后的 owner 变化都进入 write-unknown，后续队列全部停止发出。不会自动重发候选、恢复旧字节或重建默认值。
- `recover()` 是显式核对：普通 read-failed 只重读并验证；曾发出 unknown 写入时重读、flush ACK、精确再读才能确认实际观测值。不会 put 候选或旧值。尚未结束的调用由 busy 拦住，不能假装已终止后重试。
- root 当前 18 个字段严格校验；历史仅缺少 materials/visualStyle/styleDepth 时补兼容默认值。候选必须完整拥有全部 18 个字段，材质候选必须完整拥有 9 个字段。数值、颜色、语言、文本、预算及材质循环均先校验。
- 原记录未知字段按 JSON value 原始跨度保留，避免 JavaScript number 重新编码损伤大整数等未来字段。嵌套未知对象也保留原值；材质按稳定 id 合并，移除材质不会把它的未知字段转给新 id。拒绝 root 和被编辑材质对象中的重复字段，包括转义后相同的字段名。

## Index 接入

导入 `AppearancePreferences`, `AppearancePreferenceView`, `appearancePreferenceStatus`, `appearancePreferenceName`, `appearancePreferenceKey`；保留原 namespace `studio-appearance`、key `appearance-v1`，不要接入 Rust 业务数据库。

构造端口：

```ts
new AppearancePreferences({
  namespace: appearancePreferenceName,
  read: async (): Promise<string | undefined> => {
    // 必须在回调内部取得 Preferences；创建/读取失败应 throw。
    // 缺失必须由 hasSync/has 的成功结果证明。
    const store = this.appearanceStore();
    if (!store.hasSync(appearancePreferenceKey)) { return undefined; }
    const raw = store.getSync(appearancePreferenceKey, '');
    if (typeof raw !== 'string') { throw new Error('Stored appearance is not text'); }
    return raw; // existing '' 是坏记录，不能映射成缺失
  },
  put: async (raw: string): Promise<void> => {
    await this.appearanceStore().put(appearancePreferenceKey, raw);
  },
  flush: async (): Promise<void> => { await this.appearanceStore().flush(); }
}, {
  owned: (): boolean => this.pageAlive && this.foreground,
  owner: (): string => this.appearancePage + ':' + this.appearanceEpoch.toString(),
  changed: (): void => {
    // 模型已核对本次操作 owner；这里仍检查活跃页面并刷新 view/status。
    // 不能每次 callback 都将 current 覆盖到 UI；它可能属于较早 ACK。
    if (this.pageAlive && this.foreground) { this.refreshAppearanceState(); }
  }
});
```

`appearanceStore()` 必须复用同一 `preferences.Preferences` 实例并抛出打开失败；不要在 put/flush 之间换成另一实例，不要 `catch` 后假成功。初次恢复/显式恢复可重试打开失败，但不得忽略错误或把值强制 cast string。SDK 本地 `@ohos.data.preferences.d.ts` 的 put 注释明确它写到实例、flush 才持久化。

- aboutToAppear 创建单个模型并调用 `restore()`。只有成功且当前无本地候选修改时，将 view.current 应用到完整 UI。若读取期间出现本地候选，保留它并显示尚未保存。
- 原 persistAppearance 的字段采集可以保留，完整构造 AppearanceSnapshot 后调用 `save(v)`，使用 view/status 显示结果。删除旧裸 put/flush 队列，避免同一 key 两条写入路径。
- @State 可保存 AppearancePreferenceView 或至少保存 status/state/busy/dirty。`appearancePreferenceStatus(view)` 已覆盖未加载、读取、保存、读取阻断、未知和未保存预览。不要再用 `appearanceStatus || '✓ 外观设置自动保存在本机'` 作为无条件成功文案。
- read-failed/write-unknown 显示“重新读取并核对”按钮，只在 !busy 时调用 `recover()`。该动作成功只确认实际存储，不自动保存候选；若 dirty，另提供“保存当前外观”按钮显式调用 `save(view.candidate)`，并解释候选仍为预览。
- 独立 appearancePage 标识包含页面实例；每次前台/后台转换增长 appearanceEpoch，aboutToDisappear 调用 dispose。模型不会将旧响应重新绑定到新前台。foreground 到来可刷新状态；not-loaded 可再次 restore，read-failed/write-unknown 不可自动 recover，更不能自动 save。
- 日常正常保存 ACK 之后只更新状态，保持最新 UI 候选；较早 current ACK 不得回写覆盖滑块/颜色/语言等新选择。字体/纹理实际资产的安装阶段仍需其自身完整合同，本模型不把字符串字段变成资产验证。

## 真实参考与边界

- 实际 Windows `build/win-cloud-20261005/lib/main.dart:411` 的 saveContent：storageReadFailed 阻断写入，使用 pendingUiWrites 跟进实际存储结果；`storage.dart` 序列化快速变更。本模型遵守该读失败和顺序原则，不复制 Windows 聚合数据库到 HMOS UI namespace。
- 现有 Rust `shared/plugins/workbench/src/preferences.rs:333 / :400`：decode_persistent 验证原字节，encode_persistent 在验证旧字节之后合并未知字段，并按 component id 保留字段。此 HMOS JSON adapter 使用相同边界；没有声称它调用 Rust protobuf 编码。
- 外观 JSON adapter 自己限制 4 MiB，保留实际现有材质 4096/文本列表预算；这不是 SDK MAX_VALUE_LENGTH 的证明。
- 同一实际 namespace 的所有模型必须传相同 namespace 标识，默认 studio-appearance。当前模块在同进程内共享 live operation owner，dispose 不释放尚未结束的 platform Promise，unknown 原值随页面重建保留；新页面等待真实终止并显式核对，不能凭 cache read 认定持久成功。外部不经该模型的 writer、跨进程 CAS 或进程崩溃恢复仍未获资格。
- unknown 恢复的 flush 是平台 ACK 级证明；源码测试用受控存储端口执行真实 ETS 模型。没有断电、硬盘持久性、真实 preferences 服务、设备渲染或完整 Windows parity 证明。

## 初版冻结测试（历史 a1）

`appearance-preferences-a1-result.json`：56/56 PASS，0 fail/skip/cancel，7 个精确执行输入、4 个精确参考，前后哈希一致，385.1808 ms。日志 SHA-256 `af1865882775e53bdb0259d4d53eee38aa905f0f0a419f3c1a1c0a943ce08d66`。

- AppearancePreferences.ets：19,067 字节，SHA-256 `497158b595f7ef3d13bab3b9ba6cda9ac38b7e84884791bed4ae6ea96564f6e4`。
- appearance-preferences.test.cjs：31,078 字节，SHA-256 `0936c8e1ebfc548f5ce3ae6f951e6f7f4358cde31abb5aa6399ae2ab9efa7d71`。
- 真实 SDK TypeScript 4.9.5-r4 / Node v24.14.1 执行实际 ETS，不使用测试替身替代模型代码。端口、owner、延迟结束及 change observer 是受控 seam。
- SDK_BUILD、DEVICE_PERSISTENCE、INDEX_INTEGRATION 均为本独立报告 NOT_RUN。Root 完成实际接入后需另行建立相应资格，不能改写此历史范围。
