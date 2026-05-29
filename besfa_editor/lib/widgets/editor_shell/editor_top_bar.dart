import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

class EditorTopBar extends StatelessWidget {
  const EditorTopBar({
    required this.runtimeStatus,
    required this.isRuntimeBusy,
    required this.onRunPreview,
    required this.onStopPreview,
    required this.onReloadRuntime,
    super.key,
  });

  final RuntimePreviewStatus runtimeStatus;
  final bool isRuntimeBusy;
  final VoidCallback onRunPreview;
  final VoidCallback onStopPreview;
  final VoidCallback onReloadRuntime;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final statusColor = runtimeStatus.color(colorScheme);

    return Container(
      height: 52,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: colorScheme.surface,
        border: Border(
          bottom: BorderSide(color: Theme.of(context).dividerColor),
        ),
      ),
      child: Row(
        children: [
          const Icon(Icons.two_wheeler, size: 22),
          const SizedBox(width: 10),
          Text('Besfa', style: Theme.of(context).textTheme.titleMedium),
          const Spacer(),
          Icon(Icons.circle, size: 10, color: statusColor),
          const SizedBox(width: 8),
          Text(
            runtimeStatus.label,
            style: Theme.of(context).textTheme.labelMedium?.copyWith(
              color: statusColor,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: 12),
          IconButton(
            tooltip: 'Open project',
            onPressed: () {},
            icon: const Icon(Icons.folder_open),
          ),
          IconButton(
            tooltip: 'Run preview',
            onPressed: isRuntimeBusy ? null : onRunPreview,
            icon: const Icon(Icons.play_arrow),
          ),
          IconButton(
            tooltip: 'Stop preview',
            onPressed: isRuntimeBusy ? null : onStopPreview,
            icon: const Icon(Icons.stop),
          ),
          IconButton(
            tooltip: 'Reload runtime',
            onPressed: isRuntimeBusy ? null : onReloadRuntime,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
    );
  }
}

extension on RuntimePreviewStatus {
  Color color(ColorScheme colorScheme) {
    return switch (this) {
      RuntimePreviewStatus.stopped => colorScheme.outline,
      RuntimePreviewStatus.running => colorScheme.primary,
      RuntimePreviewStatus.failed => colorScheme.error,
    };
  }
}
