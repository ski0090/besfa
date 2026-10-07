import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

/// Shows the game's viewport texture and forwards the pointer to the game,
/// in texture pixels, as `pointer` commands: the scene view's camera,
/// click selection and handles run in the game.
class SceneViewport extends StatefulWidget {
  const SceneViewport({
    super.key,
    required this.textureId,
    required this.textureSize,
    required this.onCommand,
  });

  final int textureId;

  /// The texture's size in pixels, which the game's coordinates are in.
  final Size textureSize;
  final ValueChanged<Map<String, Object?>> onCommand;

  @override
  State<SceneViewport> createState() => _SceneViewportState();
}

class _SceneViewportState extends State<SceneViewport> {
  /// The button held down; others are ignored until it is released.
  String? _pressed;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = constraints.biggest;
        void pointer(String event, Offset position, [String? button]) =>
            widget.onCommand({
              'command': 'pointer',
              'event': event,
              'x': position.dx * widget.textureSize.width / size.width,
              'y': position.dy * widget.textureSize.height / size.height,
              'button': ?button,
            });
        void release(PointerEvent event) {
          if (_pressed case final button?) {
            _pressed = null;
            pointer('up', event.localPosition, button);
          }
        }

        return Listener(
          onPointerDown: (event) {
            final button = switch (event.buttons) {
              kPrimaryMouseButton => 'left',
              kMiddleMouseButton => 'middle',
              kSecondaryMouseButton => 'right',
              _ => null,
            };
            if (button != null && _pressed == null) {
              _pressed = button;
              pointer('down', event.localPosition, button);
            }
          },
          onPointerMove: (event) => pointer('move', event.localPosition),
          onPointerHover: (event) => pointer('move', event.localPosition),
          onPointerUp: release,
          onPointerCancel: release,
          onPointerSignal: (event) {
            if (event is PointerScrollEvent) {
              widget.onCommand({
                'command': 'pointer',
                'event': 'scroll',
                'delta': event.scrollDelta.dy,
              });
            }
          },
          child: Texture(textureId: widget.textureId),
        );
      },
    );
  }
}
