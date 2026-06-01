import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

/// Central Scene View surface backed by the editor-owned runtime.
class EditorViewport extends StatelessWidget {
  const EditorViewport({
    required this.platformVersion,
    required this.abiVersion,
    required this.runtimeStatus,
    required this.runtimeMessage,
    required this.frameStats,
    required this.previewTextureId,
    required this.onPickViewport,
    super.key,
  });

  final Future<String?> platformVersion;
  final int abiVersion;
  final RuntimePreviewStatus runtimeStatus;
  final String? runtimeMessage;
  final RuntimeFrameStats? frameStats;

  /// Flutter texture id for the native preview surface, when available.
  final int? previewTextureId;

  /// Called with normalized coordinates when the preview surface is clicked.
  final void Function(double viewportX, double viewportY) onPickViewport;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFF171A1A),
      child: Stack(
        children: [
          Positioned.fill(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Center(
                child: AspectRatio(
                  aspectRatio: 16 / 9,
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTapDown: previewTextureId == null
                            ? null
                            : (details) {
                                final size = constraints.biggest;
                                if (size.width <= 0 || size.height <= 0) {
                                  return;
                                }

                                onPickViewport(
                                  (details.localPosition.dx / size.width)
                                      .clamp(0, 1)
                                      .toDouble(),
                                  (details.localPosition.dy / size.height)
                                      .clamp(0, 1)
                                      .toDouble(),
                                );
                              },
                        child: Stack(
                          children: [
                            Positioned.fill(
                              child: DecoratedBox(
                                decoration: BoxDecoration(
                                  color: const Color(0xFF101314),
                                  border: Border.all(
                                    color: const Color(0xFF303637),
                                  ),
                                ),
                                child: previewTextureId == null
                                    ? Center(
                                        child: Text(
                                          _placeholderText(),
                                          style: Theme.of(context)
                                              .textTheme
                                              .bodyMedium
                                              ?.copyWith(color: Colors.white70),
                                        ),
                                      )
                                    : Texture(textureId: previewTextureId!),
                              ),
                            ),
                            const Positioned(
                              left: 12,
                              top: 12,
                              child: IgnorePointer(child: _ViewportAxisGizmo()),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
          ),
          Positioned(
            left: 16,
            right: 16,
            bottom: 12,
            child: FutureBuilder<String?>(
              future: platformVersion,
              builder: (context, snapshot) {
                final platform = snapshot.data ?? 'platform pending';
                final runtimeText = runtimeMessage == null
                    ? runtimeStatus.label
                    : '${runtimeStatus.label}: $runtimeMessage';
                final statsText = frameStats == null
                    ? ''
                    : ' | ${frameStats!.fps.toStringAsFixed(0)} FPS';
                return Text(
                  '$platform | Rust ABI $abiVersion | $runtimeText$statsText',
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: const Color(0xDBFFFFFF),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  String _placeholderText() {
    return switch (runtimeStatus) {
      RuntimePreviewStatus.stopped => 'Scene runtime offline',
      RuntimePreviewStatus.starting => 'Starting scene runtime',
      RuntimePreviewStatus.running => 'Waiting for scene surface',
      RuntimePreviewStatus.failed => 'Scene runtime failed',
    };
  }
}

class _ViewportAxisGizmo extends StatelessWidget {
  const _ViewportAxisGizmo();

  @override
  Widget build(BuildContext context) {
    return Semantics(
      label: 'World axis gizmo',
      child: const SizedBox(
        key: ValueKey('viewportAxisGizmo'),
        width: 96,
        height: 80,
        child: CustomPaint(painter: _ViewportAxisGizmoPainter()),
      ),
    );
  }
}

class _ViewportAxisGizmoPainter extends CustomPainter {
  const _ViewportAxisGizmoPainter();

  static const Color _xColor = Color(0xFFF04438);
  static const Color _yColor = Color(0xFF22C55E);
  static const Color _zColor = Color(0xFF3B82F6);

  @override
  void paint(Canvas canvas, Size size) {
    final origin = Offset(size.width * 0.42, size.height * 0.62);
    final axes = [
      _AxisGlyph(
        label: 'X',
        end: origin + Offset(size.width * 0.34, size.height * 0.16),
        color: _xColor,
      ),
      _AxisGlyph(
        label: 'Y',
        end: origin - Offset(0, size.height * 0.42),
        color: _yColor,
      ),
      _AxisGlyph(
        label: 'Z',
        end: origin + Offset(-size.width * 0.26, size.height * 0.16),
        color: _zColor,
      ),
    ];

    final shadow = Paint()
      ..color = const Color(0x99000000)
      ..strokeCap = StrokeCap.round
      ..strokeWidth = 5;
    final stroke = Paint()
      ..strokeCap = StrokeCap.round
      ..strokeWidth = 3;

    for (final axis in axes) {
      canvas.drawLine(origin, axis.end, shadow);
    }

    for (final axis in axes) {
      stroke.color = axis.color;
      canvas.drawLine(origin, axis.end, stroke);
      _drawArrowHead(canvas, origin, axis.end, axis.color);
      _drawLabel(canvas, axis.label, axis.end, axis.color);
    }

    canvas.drawCircle(origin, 4, Paint()..color = const Color(0xFFE5E7EB));
  }

  void _drawArrowHead(Canvas canvas, Offset origin, Offset end, Color color) {
    final direction = end - origin;
    final length = direction.distance;
    if (length == 0) {
      return;
    }

    final unit = direction / length;
    final normal = Offset(-unit.dy, unit.dx);
    final base = end - unit * 10;
    final path = Path()
      ..moveTo(end.dx, end.dy)
      ..lineTo((base + normal * 5).dx, (base + normal * 5).dy)
      ..lineTo((base - normal * 5).dx, (base - normal * 5).dy)
      ..close();

    canvas.drawPath(path, Paint()..color = color);
  }

  void _drawLabel(Canvas canvas, String label, Offset end, Color color) {
    final painter = TextPainter(
      text: TextSpan(
        text: label,
        style: TextStyle(
          color: color,
          fontSize: 12,
          fontWeight: FontWeight.w800,
          shadows: const [Shadow(color: Color(0xCC000000), blurRadius: 3)],
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();

    painter.paint(canvas, end + const Offset(5, -18));
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class _AxisGlyph {
  const _AxisGlyph({
    required this.label,
    required this.end,
    required this.color,
  });

  final String label;
  final Offset end;
  final Color color;
}
