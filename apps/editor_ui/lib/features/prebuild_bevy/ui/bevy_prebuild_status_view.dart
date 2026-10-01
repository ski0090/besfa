import 'package:flutter/material.dart';

import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';

/// One line with a spinner, check or error icon and the prebuild's message.
class BevyPrebuildStatusView extends StatelessWidget {
  const BevyPrebuildStatusView(this.prebuild, {super.key});

  final BevyPrebuild prebuild;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return ValueListenableBuilder(
      valueListenable: prebuild,
      builder: (context, status, _) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          switch (status.phase) {
            BevyPrebuildPhase.running => const SizedBox.square(
              dimension: 12,
              child: CircularProgressIndicator(strokeWidth: 1.5),
            ),
            BevyPrebuildPhase.ready => Icon(
              Icons.check_circle_outline,
              size: 14,
              color: colors.primary,
            ),
            BevyPrebuildPhase.failed => Icon(
              Icons.error_outline,
              size: 14,
              color: colors.error,
            ),
          },
          const SizedBox(width: 8),
          Flexible(
            child: Text(
              status.message,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 12, color: Color(0xFF7E8795)),
            ),
          ),
        ],
      ),
    );
  }
}
