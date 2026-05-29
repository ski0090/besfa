/// Descriptor for a runtime-owned preview surface that Flutter can display.
class BesfaPreviewSurfaceDescriptor {
  /// Creates a descriptor for a native shared preview surface.
  const BesfaPreviewSurfaceDescriptor({
    required this.sharedHandleName,
    required this.width,
    required this.height,
    required this.format,
  });

  /// Builds a descriptor from a JSON-like map.
  factory BesfaPreviewSurfaceDescriptor.fromMap(Map<String, Object?> map) {
    return BesfaPreviewSurfaceDescriptor(
      sharedHandleName: map['shared_handle_name'] as String? ?? '',
      width: (map['width'] as num?)?.toInt() ?? 0,
      height: (map['height'] as num?)?.toInt() ?? 0,
      format: map['format'] as String? ?? '',
    );
  }

  /// Native shared handle name opened by the Windows plugin.
  final String sharedHandleName;

  /// Surface width in physical pixels.
  final int width;

  /// Surface height in physical pixels.
  final int height;

  /// Texture format name.
  final String format;

  /// Converts this descriptor to method-channel arguments.
  Map<String, Object?> toMap() {
    return {
      'shared_handle_name': sharedHandleName,
      'width': width,
      'height': height,
      'format': format,
    };
  }
}
