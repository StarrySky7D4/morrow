import 'package:morrow_i18n/morrow_i18n.dart';

/// Compatibility display mapping for trusted host v1 diagnostics only.
/// Do not apply this to plugin literals or user content.
String? hostStorageNotice(AppLocalizations l, String detail) =>
    switch (detail) {
      "内容库保护密钥缺失，请恢复原 .audit-key 文件后重试。" => l.recoveryMissingKey,
      "内容库保护密钥不匹配或无法解密，请使用原文件及原系统账户。" => l.recoveryKeyMismatch,
      "保护密钥仍在，但内容库缺失或为空，请恢复原内容库。" => l.recoveryMissingLibrary,
      "同一内容库身份的另一份副本正在使用中，请先关闭原工作台再打开此副本。" => l.recoveryIdentityBusy,
      "此内容库正在由另一个进程使用，请关闭另一个窗口后重试。" => l.recoveryBusy,
      "恢复目标已存在，请选择尚不存在的新目录。" => l.recoveryTargetExists,
      "内容库恢复结果需要核对，请检查目标目录；原内容库未被替换。" => l.recoverySnapshotUnknown,
      "内容库备份格式或校验不正确，请保留原备份文件。" => l.recoverySnapshotInvalid,
      "备份位置已有文件，请选择新的文件名。" => l.recoveryBackupExists,
      "备份结果需要核对，请保留当前文件并检查保存位置。" => l.recoveryBackupUnknown,
      "此内容库尚未绑定保护文件，无法核对所选文件的归属。" => l.recoveryBindingMissing,
      "恢复结果需要核对，请重试打开；原保护文件副本已保留（若此前存在）。" => l.recoveryKeyUnknown,
      "内容库无法验证或打开，请保留原内容库与保护密钥后重试。" => l.recoveryLibraryInvalid,
      "工作台仍在运行，请先关闭后再切换内容库。" => l.recoveryCloseFirst,
      "已登记的内容库身份不匹配，请保留原资料并选择正确备份恢复。" => l.recoveryIdentityMismatch,
      "活动内容库登记已损坏或不受支持，已停止打开以保护资料。" => l.recoveryRegistryInvalid,
      "内容库切换结果尚未确认，请重新打开工作台核对。" => l.recoverySwitchUnknown,
      "活动内容库或登记文件无法读取，请检查原位置；不会自动创建替代内容库。" => l.recoveryRegistryUnreadable,
      _ => null,
    };
