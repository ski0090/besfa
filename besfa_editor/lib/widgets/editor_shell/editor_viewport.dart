import 'dart:async';

import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Editor Scene View camera navigation input collected from the viewport.
class EditorCameraInput {
  const EditorCameraInput({
    this.rotateDeltaX = 0,
    this.rotateDeltaY = 0,
    this.moveForward = 0,
    this.moveRight = 0,
    this.moveUp = 0,
    this.speedMultiplier = 1,
    this.deltaSeconds = 0,
  });

  /// Horizontal pointer movement in logical pixels.
  final double rotateDeltaX;

  /// Vertical pointer movement in logical pixels.
  final double rotateDeltaY;

  /// Local forward movement intent.
  final double moveForward;

  /// Local right movement intent.
  final double moveRight;

  /// World-up movement intent.
  final double moveUp;

  /// Movement speed multiplier for accelerated camera motion.
  final double speedMultiplier;

  /// Elapsed time represented by movement input, in seconds.
  final double deltaSeconds;

  /// Whether this input changes the editor camera.
  bool get hasMotion {
    return rotateDeltaX != 0 ||
        rotateDeltaY != 0 ||
        moveForward != 0 ||
        moveRight != 0 ||
        moveUp != 0;
  }
}

/// Called when the viewport gathers editor camera navigation input.
typedef EditorCameraInputHandler = void Function(EditorCameraInput input);

/// Central Scene View surface backed by the editor-owned runtime.
class EditorViewport extends StatefulWidget {
  const EditorViewport({
    required this.platformVersion,
    required this.abiVersion,
    required this.runtimeStatus,
    required this.runtimeMessage,
    required this.frameStats,
    required this.previewTextureId,
    required this.onPickViewport,
    required this.onEditorCameraInput,
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

  /// Called with Unity-style editor camera navigation input.
  final EditorCameraInputHandler onEditorCameraInput;

  @override
  State<EditorViewport> createState() => _EditorViewportState();
}

class _EditorViewportState extends State<EditorViewport> {
  static const Duration _movementTick = Duration(milliseconds: 16);
  static const double _scrollMoveSecondsPerPixel = 1 / 1200;
  static const double _boostMultiplier = 4;

  final FocusNode _focusNode = FocusNode(debugLabel: 'Scene View');
  final Set<LogicalKeyboardKey> _pressedKeys = <LogicalKeyboardKey>{};
  Timer? _movementTimer;
  DateTime? _lastMovementTickAt;
  bool _isSecondaryButtonDown = false;

  @override
  void didUpdateWidget(EditorViewport oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.previewTextureId == null) {
      _stopMovementTimer();
      _isSecondaryButtonDown = false;
    }
  }

  @override
  void dispose() {
    _stopMovementTimer();
    _focusNode.dispose();
    super.dispose();
  }

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
                      return Focus(
                        focusNode: _focusNode,
                        onKeyEvent: _handleKeyEvent,
                        child: Listener(
                          behavior: HitTestBehavior.opaque,
                          onPointerDown: _handlePointerDown,
                          onPointerMove: _handlePointerMove,
                          onPointerUp: _handlePointerUp,
                          onPointerCancel: _handlePointerCancel,
                          onPointerSignal: _handlePointerSignal,
                          child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            onTapDown: widget.previewTextureId == null
                                ? null
                                : (details) {
                                    final size = constraints.biggest;
                                    if (size.width <= 0 || size.height <= 0) {
                                      return;
                                    }

                                    widget.onPickViewport(
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
                                    child: widget.previewTextureId == null
                                        ? Center(
                                            child: Text(
                                              _placeholderText(),
                                              style: Theme.of(context)
                                                  .textTheme
                                                  .bodyMedium
                                                  ?.copyWith(
                                                    color: Colors.white70,
                                                  ),
                                            ),
                                          )
                                        : Texture(
                                            textureId: widget.previewTextureId!,
                                          ),
                                  ),
                                ),
                                const Positioned(
                                  left: 12,
                                  top: 12,
                                  child: IgnorePointer(
                                    child: _ViewportAxisGizmo(),
                                  ),
                                ),
                              ],
                            ),
                          ),
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
              future: widget.platformVersion,
              builder: (context, snapshot) {
                final platform = snapshot.data ?? 'platform pending';
                final runtimeText = widget.runtimeMessage == null
                    ? widget.runtimeStatus.label
                    : '${widget.runtimeStatus.label}: ${widget.runtimeMessage}';
                final statsText = widget.frameStats == null
                    ? ''
                    : ' | ${widget.frameStats!.fps.toStringAsFixed(0)} FPS';
                return Text(
                  '$platform | Rust ABI ${widget.abiVersion} | $runtimeText$statsText',
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
    return switch (widget.runtimeStatus) {
      RuntimePreviewStatus.stopped => 'Scene runtime offline',
      RuntimePreviewStatus.starting => 'Starting scene runtime',
      RuntimePreviewStatus.running => 'Waiting for scene surface',
      RuntimePreviewStatus.failed => 'Scene runtime failed',
    };
  }

  void _handlePointerDown(PointerDownEvent event) {
    _focusNode.requestFocus();
    if (widget.previewTextureId == null) {
      return;
    }

    if (_hasSecondaryButton(event.buttons)) {
      _isSecondaryButtonDown = true;
      _syncMovementTimer();
    }
  }

  void _handlePointerMove(PointerMoveEvent event) {
    if (widget.previewTextureId == null ||
        !_hasSecondaryButton(event.buttons)) {
      return;
    }

    _isSecondaryButtonDown = true;
    _emitCameraInput(
      EditorCameraInput(
        rotateDeltaX: event.localDelta.dx,
        rotateDeltaY: event.localDelta.dy,
      ),
    );
    _syncMovementTimer();
  }

  void _handlePointerUp(PointerUpEvent event) {
    if (!_hasSecondaryButton(event.buttons)) {
      _isSecondaryButtonDown = false;
      _syncMovementTimer();
    }
  }

  void _handlePointerCancel(PointerCancelEvent event) {
    _isSecondaryButtonDown = false;
    _syncMovementTimer();
  }

  void _handlePointerSignal(PointerSignalEvent event) {
    if (widget.previewTextureId == null || event is! PointerScrollEvent) {
      return;
    }

    final scrollDelta = event.scrollDelta.dy;
    if (scrollDelta == 0) {
      return;
    }

    _focusNode.requestFocus();
    _emitCameraInput(
      EditorCameraInput(
        moveForward: scrollDelta.isNegative ? 1 : -1,
        speedMultiplier: _speedMultiplier,
        deltaSeconds: (scrollDelta.abs() * _scrollMoveSecondsPerPixel)
            .clamp(0.01, 0.08)
            .toDouble(),
      ),
    );
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is KeyDownEvent || event is KeyRepeatEvent) {
      _pressedKeys.add(event.logicalKey);
    } else if (event is KeyUpEvent) {
      _pressedKeys.remove(event.logicalKey);
    }

    if (_isCameraKey(event.logicalKey)) {
      _syncMovementTimer();
      return KeyEventResult.handled;
    }

    return KeyEventResult.ignored;
  }

  void _syncMovementTimer() {
    if (widget.previewTextureId == null ||
        !_isSecondaryButtonDown ||
        !_hasMovementKeys) {
      _stopMovementTimer();
      return;
    }

    _lastMovementTickAt ??= DateTime.now();
    _movementTimer ??= Timer.periodic(_movementTick, _emitMovementTick);
  }

  void _emitMovementTick(Timer timer) {
    if (widget.previewTextureId == null ||
        !_isSecondaryButtonDown ||
        !_hasMovementKeys) {
      _stopMovementTimer();
      return;
    }

    final now = DateTime.now();
    final lastTick = _lastMovementTickAt ?? now;
    _lastMovementTickAt = now;
    final deltaSeconds =
        now.difference(lastTick).inMicroseconds /
        Duration.microsecondsPerSecond;
    _emitCameraInput(
      EditorCameraInput(
        moveForward: _forwardMovement,
        moveRight: _rightMovement,
        moveUp: _upMovement,
        speedMultiplier: _speedMultiplier,
        deltaSeconds: deltaSeconds.clamp(0.001, 0.05).toDouble(),
      ),
    );
  }

  void _stopMovementTimer() {
    _movementTimer?.cancel();
    _movementTimer = null;
    _lastMovementTickAt = null;
  }

  void _emitCameraInput(EditorCameraInput input) {
    if (widget.previewTextureId == null || !input.hasMotion) {
      return;
    }

    widget.onEditorCameraInput(input);
  }

  bool _hasSecondaryButton(int buttons) {
    return (buttons & kSecondaryMouseButton) != 0;
  }

  bool _isCameraKey(LogicalKeyboardKey key) {
    return key == LogicalKeyboardKey.keyW ||
        key == LogicalKeyboardKey.keyA ||
        key == LogicalKeyboardKey.keyS ||
        key == LogicalKeyboardKey.keyD ||
        key == LogicalKeyboardKey.keyQ ||
        key == LogicalKeyboardKey.keyE ||
        key == LogicalKeyboardKey.arrowUp ||
        key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowLeft ||
        key == LogicalKeyboardKey.arrowRight ||
        key == LogicalKeyboardKey.shiftLeft ||
        key == LogicalKeyboardKey.shiftRight;
  }

  bool get _hasMovementKeys {
    return _forwardMovement != 0 || _rightMovement != 0 || _upMovement != 0;
  }

  double get _forwardMovement {
    return _axis(
      positive: [LogicalKeyboardKey.keyW, LogicalKeyboardKey.arrowUp],
      negative: [LogicalKeyboardKey.keyS, LogicalKeyboardKey.arrowDown],
    );
  }

  double get _rightMovement {
    return _axis(
      positive: [LogicalKeyboardKey.keyD, LogicalKeyboardKey.arrowRight],
      negative: [LogicalKeyboardKey.keyA, LogicalKeyboardKey.arrowLeft],
    );
  }

  double get _upMovement {
    return _axis(
      positive: [LogicalKeyboardKey.keyE],
      negative: [LogicalKeyboardKey.keyQ],
    );
  }

  double get _speedMultiplier {
    return _pressedKeys.contains(LogicalKeyboardKey.shiftLeft) ||
            _pressedKeys.contains(LogicalKeyboardKey.shiftRight)
        ? _boostMultiplier
        : 1;
  }

  double _axis({
    required List<LogicalKeyboardKey> positive,
    required List<LogicalKeyboardKey> negative,
  }) {
    final positivePressed = positive.any(_pressedKeys.contains);
    final negativePressed = negative.any(_pressedKeys.contains);
    if (positivePressed == negativePressed) {
      return 0;
    }

    return positivePressed ? 1 : -1;
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
