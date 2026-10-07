import 'package:flutter/services.dart';

/// Bevy's `KeyCode` name for [key], which is the key's W3C code name, like
/// `KeyW`; null for keys the game is not told about. Physical, so a game's
/// WASD stays where it is on any keyboard layout.
String? bevyKeyCode(PhysicalKeyboardKey key) {
  // USB HID usages: letters, then digits from 1 to 0, then F1 to F12.
  final usage = key.usbHidUsage;
  if (usage >= _keyA && usage < _keyA + 26) {
    return 'Key${String.fromCharCode(0x41 + usage - _keyA)}';
  }
  if (usage >= _digit1 && usage < _digit1 + 10) {
    return 'Digit${(usage - _digit1 + 1) % 10}';
  }
  if (usage >= _f1 && usage < _f1 + 12) {
    return 'F${usage - _f1 + 1}';
  }
  return _named[key];
}

final _keyA = PhysicalKeyboardKey.keyA.usbHidUsage;
final _digit1 = PhysicalKeyboardKey.digit1.usbHidUsage;
final _f1 = PhysicalKeyboardKey.f1.usbHidUsage;

final _named = {
  PhysicalKeyboardKey.enter: 'Enter',
  PhysicalKeyboardKey.escape: 'Escape',
  PhysicalKeyboardKey.backspace: 'Backspace',
  PhysicalKeyboardKey.tab: 'Tab',
  PhysicalKeyboardKey.space: 'Space',
  PhysicalKeyboardKey.minus: 'Minus',
  PhysicalKeyboardKey.equal: 'Equal',
  PhysicalKeyboardKey.bracketLeft: 'BracketLeft',
  PhysicalKeyboardKey.bracketRight: 'BracketRight',
  PhysicalKeyboardKey.backslash: 'Backslash',
  PhysicalKeyboardKey.semicolon: 'Semicolon',
  PhysicalKeyboardKey.quote: 'Quote',
  PhysicalKeyboardKey.backquote: 'Backquote',
  PhysicalKeyboardKey.comma: 'Comma',
  PhysicalKeyboardKey.period: 'Period',
  PhysicalKeyboardKey.slash: 'Slash',
  PhysicalKeyboardKey.capsLock: 'CapsLock',
  PhysicalKeyboardKey.insert: 'Insert',
  PhysicalKeyboardKey.delete: 'Delete',
  PhysicalKeyboardKey.home: 'Home',
  PhysicalKeyboardKey.end: 'End',
  PhysicalKeyboardKey.pageUp: 'PageUp',
  PhysicalKeyboardKey.pageDown: 'PageDown',
  PhysicalKeyboardKey.arrowUp: 'ArrowUp',
  PhysicalKeyboardKey.arrowDown: 'ArrowDown',
  PhysicalKeyboardKey.arrowLeft: 'ArrowLeft',
  PhysicalKeyboardKey.arrowRight: 'ArrowRight',
  PhysicalKeyboardKey.shiftLeft: 'ShiftLeft',
  PhysicalKeyboardKey.shiftRight: 'ShiftRight',
  PhysicalKeyboardKey.controlLeft: 'ControlLeft',
  PhysicalKeyboardKey.controlRight: 'ControlRight',
  PhysicalKeyboardKey.altLeft: 'AltLeft',
  PhysicalKeyboardKey.altRight: 'AltRight',
  PhysicalKeyboardKey.metaLeft: 'SuperLeft',
  PhysicalKeyboardKey.metaRight: 'SuperRight',
};
