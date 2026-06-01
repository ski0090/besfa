/// Editor-facing preview runtime status.
enum RuntimePreviewStatus {
  /// Preview runtime is not running.
  stopped,

  /// Preview runtime is starting and waiting for IPC readiness.
  starting,

  /// Preview runtime is running and ready.
  running,

  /// Preview runtime failed to start, stop, or report state.
  failed;

  /// User-facing status label.
  String get label {
    return switch (this) {
      RuntimePreviewStatus.stopped => 'Offline',
      RuntimePreviewStatus.starting => 'Starting',
      RuntimePreviewStatus.running => 'Scene Ready',
      RuntimePreviewStatus.failed => 'Failed',
    };
  }
}
