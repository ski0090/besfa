enum RuntimePreviewStatus {
  stopped,
  running,
  failed;

  String get label {
    return switch (this) {
      RuntimePreviewStatus.stopped => 'Stopped',
      RuntimePreviewStatus.running => 'Running',
      RuntimePreviewStatus.failed => 'Failed',
    };
  }
}
