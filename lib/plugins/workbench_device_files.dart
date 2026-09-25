import '../attachments/attachment.dart';
import '../media/texture_source.dart';

/// Local platform streams beside the framed host protocol. Implementations must
/// use the same host import/export rules; they never upload files to a service.
abstract interface class WorkbenchDeviceFiles {
  Future<IdeaAttachment> importDeviceFile(String card, IdeaAttachment selected);
  Future<DeviceFilePreview> exportDeviceFile(
    String card,
    String asset,
    String name,
    TextureKind kind,
  );
  bool hasDevicePreview(TextureSource source);
  void releaseDevicePreview(TextureSource source);
  void releaseDevicePreviews();
}

class DeviceFilePreview {
  const DeviceFilePreview(this.source, this.bytes, this.sha256);
  final TextureSource source;
  final BigInt bytes;
  final String sha256;
}
