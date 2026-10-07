import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';

import 'support/editor_harness.dart';

const _cube = 4294967295;
const _transform = 'bevy_transform::components::transform::Transform';

String _transformValue(double x) =>
    '{"translation":[$x,0.0,0.0],"rotation":[0.0,0.0,0.0,1.0],'
    '"scale":[1.0,1.0,1.0]}';

Map<String, Object?> _set(double x) => {
  'command': 'set',
  'id': _cube,
  'component': _transform,
  'value': {
    'translation': [x, 0, 0],
    'rotation': [0, 0, 0, 1],
    'scale': [1, 1, 1],
  },
};

Future<void> _control(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pump();
}

void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('besfa_convenience'));
  tearDown(() => dir.deleteSync(recursive: true));

  Future<(FakeGame, void Function(String))> openWithCube(
    WidgetTester tester,
  ) async {
    final (game, output) = await openEditor(tester, dir);
    output(
      '@besfa {"type":"entities","entities":['
      '{"id":$_cube,"name":"Cube","parent":null,"scene":true}]}',
    );
    await tester.pump();
    await tester.tap(find.text('Cube'));
    await tester.pump();
    output(
      '@besfa {"type":"entity","id":$_cube,"components":['
      '{"name":"Transform","path":"$_transform","mutable":true,"saved":true,'
      '"value":${_transformValue(0)}}]}',
    );
    await tester.pumpAndSettle();
    return (game, output);
  }

  testWidgets('despawns a deleted entity once no undo can restore it', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openWithCube(tester);
    output('@besfa {"type":"spawned","id":42}');
    await tester.pump();
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, {'command': 'delete', 'id': 42});

    // A new change forgets the undone spawn: nothing can restore 42 now.
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    expect(game.commands.reversed.take(2), [
      {'command': 'despawn', 'id': 42},
      {'command': 'delete', 'id': _cube},
    ]);

    // The cube's deletion can still be undone, so the cube stays hidden.
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, {'command': 'restore', 'id': _cube});
    expect(game.commands.where((c) => c['command'] == 'despawn'), hasLength(1));
  });

  testWidgets('undoes and redoes field edits, drags, adds and deletes', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openWithCube(tester);

    // A field edit: undo sets the value back, redo sets it again.
    final x = find.byType(TextField).first;
    await tester.tap(x);
    await tester.enterText(x, '3');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(game.commands.last, _set(3));
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, _set(0));
    await _control(tester, LogicalKeyboardKey.keyY);
    expect(game.commands.last, _set(3));

    // A drag in the scene view.
    output(
      '@besfa {"type":"edited","id":$_cube,"component":"$_transform",'
      '"before":${_transformValue(3)},"after":${_transformValue(5)}}',
    );
    await tester.pump();
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, _set(3));

    // Something the game spawned for the editor is deleted again.
    output('@besfa {"type":"spawned","id":42}');
    await tester.pump();
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, {'command': 'delete', 'id': 42});
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(game.commands.last, {'command': 'restore', 'id': 42});

    // Deleting hides the entity; undo brings the same one back.
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    expect(game.commands.last, {'command': 'delete', 'id': _cube});
    await tester.tap(find.text('Edit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Undo'));
    await tester.pumpAndSettle();
    expect(game.commands.last, {'command': 'restore', 'id': _cube});

    // Removing a component puts it back with its value on undo: the last
    // one sent, since undoing the drag, even before the game reports it.
    await tester.tap(find.byTooltip('Remove component'));
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, _set(3));

    // Removing one without a value records nothing to undo: the next undo
    // takes back the entity the game spawned earlier.
    output(
      '@besfa {"type":"entity","id":$_cube,"components":['
      '{"name":"Spin","path":"demo::Spin","mutable":true,"saved":true}]}',
    );
    await tester.pump();
    await tester.tap(find.byTooltip('Remove component'));
    final sent = game.sent.length;
    await _control(tester, LogicalKeyboardKey.keyZ);
    expect(game.commands.last, {'command': 'delete', 'id': 42});
    expect(game.sent, hasLength(sent + 1));
  });

  testWidgets('places assets and saves prefabs', (WidgetTester tester) async {
    for (final file in [
      'assets/models/tree.glb',
      'assets/prefabs/rock.scn.ron',
      'assets/textures/bark.png',
    ]) {
      File('${dir.path}/$file').createSync(recursive: true);
    }
    final (game, _) = await openWithCube(tester);

    await tester.tap(find.text('Assets'));
    await tester.pump();
    for (final name in ['models', 'tree.glb', 'prefabs', 'rock.scn.ron']) {
      expect(find.text(name), findsOneWidget);
    }
    // Only models, scenes and prefabs can go in the scene.
    expect(find.byTooltip('Add to scene'), findsNWidgets(2));
    await tester.tap(find.byTooltip('Add to scene').first);
    expect(game.commands.last, {
      'command': 'instantiate',
      'path': 'models/tree.glb',
    });

    await tester.tap(find.byTooltip('Save as prefab'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Cube'), 'bad/name');
    await tester.pump();
    expect(find.text('Letters, digits, spaces, - and _ only'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Save'));
    await tester.pumpAndSettle();
    expect(find.text('Save as prefab'), findsWidgets, reason: 'still open');
    await tester.enterText(find.byType(TextField).last, 'Big cube');
    await tester.tap(find.widgetWithText(FilledButton, 'Save'));
    await tester.pumpAndSettle();
    expect(game.commands.last, {
      'command': 'save_prefab',
      'id': _cube,
      'path': 'prefabs/Big cube.scn.ron',
    });
  });

  testWidgets('a code change saves, then rebuilds the edit session', (
    WidgetTester tester,
  ) async {
    final main = File('${dir.path}/src/main.rs')
      ..createSync(recursive: true)
      ..writeAsStringSync('fn main() {}');
    mockEditorChannels(tester);
    final games = <FakeGame>[];
    final outputs = <void Function(String)>[];
    await tester.pumpWidget(
      MaterialApp(
        home: ProjectEditorPage(
          project: Project(dir.path),
          startGame: (_, {required environment, required onOutput}) async {
            games.add(FakeGame());
            outputs.add(onOutput);
            return games.last;
          },
        ),
      ),
    );
    await tester.pump();
    expect(games, hasLength(1));
    // An unsaved change, so the rebuild saves first.
    await tester.tap(find.byTooltip('Add entity'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cube'));
    await tester.pumpAndSettle();

    await tester.runAsync(() async {
      main.writeAsStringSync('fn main() { println!("changed"); }');
      // Let the file system report the change.
      await Future<void>.delayed(const Duration(seconds: 1));
    });
    await tester.pump(const Duration(milliseconds: 600));
    expect(games.first.commands.last, {'command': 'save'});
    expect(games, hasLength(1), reason: 'waits for the save');

    outputs.first('@besfa {"type":"saved","error":null}');
    await tester.pumpAndSettle();
    expect(games, hasLength(2));
    expect(find.text('Source changed; rebuilding.'), findsOneWidget);
  });
}
