import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:editor_ui/widgets/scene_viewport/lib/bevy_key.dart';

final _flyKeys = {
  PhysicalKeyboardKey.keyW,
  PhysicalKeyboardKey.keyA,
  PhysicalKeyboardKey.keyS,
  PhysicalKeyboardKey.keyD,
  PhysicalKeyboardKey.keyQ,
  PhysicalKeyboardKey.keyE,
};

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
    this.onLooking,
  });

  final int textureId;

  /// The texture's size in pixels, which the game's coordinates are in.
  final Size textureSize;
  final ValueChanged<Map<String, Object?>> onCommand;

  /// The game plays: keys pressed while the viewport has the keyboard go to
  /// it as `key` commands. Play and clicking the viewport give it the
  /// keyboard; clicking elsewhere takes it away and lets held keys go.
  final bool playing;

  /// The right button went down or up in edit mode. While it is held the
  /// viewport's keys fly the scene view, so the editor's shortcuts wait.
  final ValueChanged<bool>? onLooking;

  @override
  State<SceneViewport> createState() => _SceneViewportState();
}

class _SceneViewportState extends State<SceneViewport> {
  /// The button held down; others are ignored until it is released.
  String? _pressed;

  final _focus = FocusNode(debugLabel: 'Scene viewport');

  /// Fly keys held while looking around in edit mode.
  final _held = <PhysicalKeyboardKey>{};

  bool get _looking => _pressed == 'right' && !widget.playing;

  @override
  void didUpdateWidget(SceneViewport old) {
    super.didUpdateWidget(old);
    if (widget.playing != old.playing) {
      _held.clear();
    }
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
    if (!widget.playing) {
      return _looking ? _fly(event) : KeyEventResult.ignored;
    }
    final code = bevyKeyCode(event.physicalKey);
    if (code == null) {
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

  /// While the right button is held in edit mode, W, A, S and D fly the
  /// scene view's camera, Q and E down and up, and Shift speeds it up. They
  /// stay the editor's keys: the game's input never sees them.
  KeyEventResult _fly(KeyEvent event) {
    final key = event.physicalKey;
    final shift =
        key == PhysicalKeyboardKey.shiftLeft ||
        key == PhysicalKeyboardKey.shiftRight;
    if (_flyKeys.contains(key) || shift) {
      if (event is KeyDownEvent) {
        _held.add(key);
      } else if (event is KeyUpEvent) {
        _held.remove(key);
      }
      if (event is! KeyRepeatEvent) {
        _sendFly();
      }
    }
    return KeyEventResult.handled;
  }

  void _sendFly() {
    int axis(PhysicalKeyboardKey plus, PhysicalKeyboardKey minus) =>
        (_held.contains(plus) ? 1 : 0) - (_held.contains(minus) ? 1 : 0);
    widget.onCommand({
      'command': 'fly',
      'right': axis(PhysicalKeyboardKey.keyD, PhysicalKeyboardKey.keyA),
      'up': axis(PhysicalKeyboardKey.keyE, PhysicalKeyboardKey.keyQ),
      'forward': axis(PhysicalKeyboardKey.keyW, PhysicalKeyboardKey.keyS),
      'fast':
          _held.contains(PhysicalKeyboardKey.shiftLeft) ||
          _held.contains(PhysicalKeyboardKey.shiftRight),
    });
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
            final looked = _looking;
            _pressed = null;
            // The game stops flying when the button goes up.
            pointer('up', event.localPosition, button);
            if (looked) {
              _held.clear();
              widget.onLooking?.call(false);
            }
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
              if (_looking) {
                widget.onLooking?.call(true);
              }
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
              if (focused) {
                return;
              }
              if (widget.playing) {
                widget.onCommand({'command': 'focus_lost'});
              } else if (_held.isNotEmpty) {
                _held.clear();
                _sendFly();
              }
            },
            child: view,
          ),
        );
      },
    );
  }
}
