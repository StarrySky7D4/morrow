import 'package:morrow_i18n/morrow_i18n.dart';

/// Exact compatibility keys for existing native/guest diagnostic messages.
/// This maps known host notices only, never pasted content, file names, formulas
/// or arbitrary third-party messages. ARB fragments own their displayed wording.
String localizeImportNotice(
  String notice,
  AppLocalizations messages,
) => switch (notice) {
  '剪贴板文件超过 200 MB。' => messages.importsFileTooLarge,
  '此环境不支持读取剪贴板，请使用导入文件。' => messages.importsUnsupported,
  '最多导入 20 个附件，其余内容请分次粘贴。' => messages.importsAttachmentLimit,
  '本次粘贴的文件总量超过 200 MB，请分次导入。' => messages.importsTotalTooLarge,
  '文本超过 2 MB，请作为文件导入。' => messages.importsTextTooLarge,
  '富文本排版无法完整转换，已保留可读文字。' => messages.importsRichFallback,
  '表格已转换为 Markdown，完整数据保留在 TSV 附件中。' => messages.importsTableConverted,
  '有一项剪贴板内容无法读取，其余可读内容已保留。' => messages.importsItemUnreadable,
  '本次只读取前 20 项，请分次粘贴更多内容。' => messages.importsItemLimit,
  'Excel 原始表格已保留为 XML 附件。' => messages.importsExcelXmlKept,
  'Office 原始对象未能读取，已保留其他可用内容。' => messages.importsOfficeUnreadable,
  '读取期间剪贴板发生了变化，请重新粘贴。' => messages.importsClipboardChanged,
  '部分内嵌图片需要作为文件单独导入。' => messages.importsEmbeddedImagesSeparate,
  '有一张内嵌图片无法读取。' => messages.importsEmbeddedImageUnreadable,
  '本地链接图片没有自动读取，请粘贴图片或导入原文件。' => messages.importsLocalImageNotRead,
  '合并单元格已转为阅读表格；原始排版保留在 HTML 附件。' => messages.importsMergedTable,
  '表格显示值与公式已转为 Markdown，格式和合并信息保留在 XML 附件。' => messages.importsExcelValues,
  'Office 嵌入对象已保留为原始附件；图表、公式和版式可用原软件继续编辑。' =>
    messages.importsOfficeEmbeddedKept,
  '有一个 Office 对象超过限制或无法导出，请在原软件保存后导入。' => messages.importsOfficeExportFailed,
  'Office 剪贴板暂不可用。' => messages.importsOfficeUnavailable,
  '剪贴板正被其他应用占用，Office 对象未读取。' => messages.importsOfficeBusy,
  'Office 内容读取失败，其他剪贴板内容仍可使用。' => messages.importsOfficeReadFailed,
  '富文本超过 2 MB，请将文档作为附件导入。' => messages.importsRichTooLarge,
  '表格过大，请导入 Excel 文件。' => messages.importsSpreadsheetTooLarge,
  'RTF 过大，请导入原始文档。' => messages.importsRtfTooLarge,
  '剪贴板已变化，请重新粘贴。' || '读取期间剪贴板发生变化，请重新粘贴。' => messages.importsClipboardChanged,
  _ => notice,
};
