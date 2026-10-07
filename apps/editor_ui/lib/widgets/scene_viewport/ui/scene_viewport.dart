import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:editor_ui/widgets/scene_viewport/lib/bevy_key.dart';

/// Shows the game's viewport texture and forwards the pointer to the game,
/// in texture pixels, as `pointer` commands: the scene view's camera,
/// click selection and handles run in the game, and once it plays, the
/// pointer is its mouse.
class SceneViewport extends StatefulWidget {
  const SceneViewport({
    super.key,
    required this.textureId,
    required this.textureSize,
    required this.onCommand,
    this.playing = false,
  });

  final int textureId;

  /// The texture's size in pixels, which the game's coordinates are in.
  final Size textureSize;
  final ValueChanged<Map<String, Object?>> onCommand;

  /// The game plays: keys pressed while the viewport has the keyboard go to
  /// it as `key` commands. Play and clicking the viewport give it the
  /// keyboard; clicking elsewhere takes it away and lets held keys go.
  final bool playing;

  @override
  State<SceneViewport> createState() => _SceneViewportState();
}

class _SceneViewportState extends State<SceneViewport> {
  /// The button held down; others are ignored until it is released.
  String? _pressed;

  final _focus = FocusNode(debugLabel: 'Scene viewport');

  @override
  void didUpdateWidget(SceneViewport old) {
    super.didUpdateWidget(old);
    if (widget.playing && !old.playing) {
      _focus.requestFocus();
    }
  }

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    final code = bevyKeyCode(event.physicalKey);
    if (!widget.playing || code == null) {
      return KeyEventResult.ignored;
    }
    // A held key stays pressed in the game until it is released.
    if (event is! KeyRepeatEvent) {
      widget.onCommand({
        'command': 'key',
        'code': code,
        'pressed': event is KeyDownEvent,
        'text': ?event.character,
      });
    }
    return KeyEventResult.handled;
  }

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

        final view = Listener(
          onPointerDown: (event) {
            _focus.requestFocus();
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
        return TapRegion(
          onTapOutside: (_) => _focus.unfocus(),
          child: Focus(
            focusNode: _focus,
            autofocus: widget.playing,
            onKeyEvent: _onKey,
            onFocusChange: (focused) {
              if (!focused && widget.playing) {
                widget.onCommand({'command': 'focus_lost'});
              }
            },
            child: view,
          ),
        );
      },
    );
  }
}
