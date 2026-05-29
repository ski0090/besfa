import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

class EditorViewport extends StatelessWidget {
  const EditorViewport({
    required this.platformVersion,
    required this.abiVersion,
    required this.runtimeStatus,
    super.key,
  });

  final Future<String?> platformVersion;
  final int abiVersion;
  final RuntimePreviewStatus runtimeStatus;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFFF4F1EA),
      child: Stack(
        children: [
          Center(
            child: Container(
              width: 520,
              height: 320,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: const Color(0xFF202124),
                borderRadius: BorderRadius.circular(6),
              ),
              child: const Text(
                'Preview surface',
                style: TextStyle(color: Colors.white70),
              ),
            ),
          ),
          Positioned(
            left: 16,
            bottom: 16,
            child: FutureBuilder<String?>(
              future: platformVersion,
              builder: (context, snapshot) {
                final platform = snapshot.data ?? 'platform pending';
                return Text(
                  '$platform | Rust ABI $abiVersion | ${runtimeStatus.label}',
                  style: Theme.of(context).textTheme.bodySmall,
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
