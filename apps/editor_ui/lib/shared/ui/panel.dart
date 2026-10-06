import 'package:flutter/material.dart';

const panelText = Color(0xFFB3BBC8);
const panelMutedText = Color(0xFF7E8795);

/// A titled side panel of the editor.
class Panel extends StatelessWidget {
  const Panel({super.key, required this.title, required this.child});

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFF1A1D23),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            height: 28,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            alignment: Alignment.centerLeft,
            decoration: BoxDecoration(
              border: Border(
                bottom: BorderSide(color: Colors.white.withValues(alpha: .08)),
              ),
            ),
            child: Text(
              title,
              style: const TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w600,
                letterSpacing: .4,
                color: panelMutedText,
              ),
            ),
          ),
          Expanded(child: child),
        ],
      ),
    );
  }
}
