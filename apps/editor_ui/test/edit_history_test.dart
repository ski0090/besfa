import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/features/undo/model/edit_history.dart';

void main() {
  test('undoes, redoes, and forgets what was undone on a new change', () {
    final history = EditHistory();
    Edit edit(int n) => Edit(
      undo: [
        {'undo': n},
      ],
      redo: [
        {'redo': n},
      ],
    );
    expect(history.undo(), isNull);

    history
      ..add(edit(1))
      ..add(edit(2));
    expect(history.undo(), [
      {'undo': 2},
    ]);
    expect(history.canRedo, isTrue);
    expect(history.redo(), [
      {'redo': 2},
    ]);
    expect(history.redo(), isNull);

    history.undo();
    history.add(edit(3));
    expect(history.canRedo, isFalse);
    expect(history.undo(), [
      {'undo': 3},
    ]);
    expect(history.undo(), [
      {'undo': 1},
    ]);

    history.clear();
    expect(history.canUndo || history.canRedo, isFalse);
  });
}
