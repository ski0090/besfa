import 'package:flutter/foundation.dart';

/// A change to the scene as the game commands that make and unmake it.
class Edit {
  const Edit({required this.undo, required this.redo});

  final List<Map<String, Object?>> undo;
  final List<Map<String, Object?>> redo;
}

/// The scene changes made in this edit session, for undo and redo. The
/// commands name entities by id, which stays valid for the session: the
/// game hides deleted entities instead of despawning them.
class EditHistory extends ChangeNotifier {
  final _done = <Edit>[];
  final _undone = <Edit>[];

  bool get canUndo => _done.isNotEmpty;
  bool get canRedo => _undone.isNotEmpty;

  /// Records [edit], which forgets what was undone.
  void add(Edit edit) {
    _done.add(edit);
    _undone.clear();
    notifyListeners();
  }

  /// The commands that undo the last change, if any.
  List<Map<String, Object?>>? undo() {
    if (_done.isEmpty) {
      return null;
    }
    final edit = _done.removeLast();
    _undone.add(edit);
    notifyListeners();
    return edit.undo;
  }

  /// The commands that redo the last undone change, if any.
  List<Map<String, Object?>>? redo() {
    if (_undone.isEmpty) {
      return null;
    }
    final edit = _undone.removeLast();
    _done.add(edit);
    notifyListeners();
    return edit.redo;
  }

  /// A new game process: the ids belong to the old one.
  void clear() {
    _done.clear();
    _undone.clear();
    notifyListeners();
  }
}
