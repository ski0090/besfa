import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/widgets/entity_inspector/ui/value_editor.dart';

import 'support/editor_harness.dart';

const _cube = 4294967295;
const _transform = 'bevy_transform::components::transform::Transform';

/// The game's report of the cube with its Transform at [translation].
String _cubeReport(List<double> translation) =>
    '@besfa {"type":"entity","id":$_cube,"components":['
    '{"name":"Transform","path":"$_transform","mutable":true,"saved":true,'
    '"value":{"translation":$translation,"rotation":[0.0,0.0,0.0,1.0],'
    '"scale":[1.0,1.0,1.0]}}]}';

void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('besfa_editing'));
  tearDown(() => dir.deleteSync(recursive: true));

  /// The editor with the cube selected and its Transform shown.
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
    output(_cubeReport([0.0, 1.0, 0.0]));
    await tester.pumpAndSettle();
    return (game, output);
  }

  /// Translation x, y, z, rotation X, Y, Z, then scale x, y, z.
  Finder field(int index) => find.byType(TextField).at(index);

  testWidgets('a field keeps what is typed and sends the whole value', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openWithCube(tester);
    expect(find.textContaining(' *'), findsNothing);

    await tester.tap(field(0));
    await tester.enterText(field(0), '2.5');
    // The game moves the cube while the field is being typed in.
    output(_cubeReport([0.0, 7.0, 0.0]));
    await tester.pump();
    expect(tester.widget<TextField>(field(0)).controller!.text, '2.5');
    expect(tester.widget<TextField>(field(1)).controller!.text, '7');

    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();

    expect(game.commands.last, {
      'command': 'set',
      'id': _cube,
      'component': _transform,
      'value': {
        'translation': [2.5, 7, 0],
        'rotation': [0, 0, 0, 1],
        'scale': [1, 1, 1],
      },
    });
    expect(find.textContaining(' *'), findsOneWidget, reason: 'unsaved');

    // Rotation is edited as degrees.
    expect(tester.widget<TextField>(field(4)).controller!.text, '0');
    await tester.tap(field(4));
    await tester.enterText(field(4), '90');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    final rotation =
        (game.commands.last['value']! as Map)['rotation'] as List<Object?>;
    expect(rotation, [
      closeTo(0, 1e-9),
      closeTo(0.7071068, 1e-6),
      closeTo(0, 1e-9),
      closeTo(0.7071068, 1e-6),
    ]);

    // Saving clears the mark once the game says it worked.
    output('@besfa {"type":"saved","error":null}');
    await tester.pump();
    expect(find.textContaining(' *'), findsNothing);
  });

  testWidgets('an unchanged or unreadable number sends nothing', (
    WidgetTester tester,
  ) async {
    final (game, _) = await openWithCube(tester);
    final sent = game.sent.length;

    await tester.tap(field(1));
    await tester.enterText(field(1), '1.0000');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.tap(field(2));
    await tester.enterText(field(2), 'far');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();

    expect(game.sent, hasLength(sent));
    expect(tester.widget<TextField>(field(2)).controller!.text, '0');
  });

  testWidgets('adds, duplicates and deletes from the hierarchy and keys', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openWithCube(tester);

    await tester.tap(find.byTooltip('Add entity'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sphere'));
    await tester.pumpAndSettle();
    expect(game.commands.last, {'command': 'spawn', 'kind': 'sphere'});

    // The game selects what it spawned.
    output(
      '@besfa {"type":"entities","entities":['
      '{"id":$_cube,"name":"Cube","parent":null,"scene":true},'
      '{"id":7,"name":"Sphere","parent":null,"scene":true}]}',
    );
    output('@besfa {"type":"selected","id":7}');
    await tester.pump();
    expect(find.text('Sphere'), findsNWidgets(2), reason: 'row and header');

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyD);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(game.commands.last, {'command': 'duplicate', 'id': 7});

    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    expect(game.commands.last, {'command': 'despawn', 'id': 7});

    // Delete in a text field edits the text, not the scene.
    output(
      '@besfa {"type":"entity","id":7,"components":['
      '{"name":"Name","path":"bevy_ecs::name::Name","mutable":true,'
      '"saved":true,"value":"Sphere"}]}',
    );
    await tester.pump();
    final sent = game.sent.length;
    await tester.tap(find.byType(TextField));
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    expect(game.sent, hasLength(sent));
  });

  testWidgets('adds and removes components', (WidgetTester tester) async {
    final (game, output) = await openWithCube(tester);
    output(
      '@besfa {"type":"components","components":['
      '{"name":"MeshShape","path":"besfa_editor_plugin::scene::MeshShape"},'
      '{"name":"Transform","path":"$_transform"},'
      '{"name":"PointLight","path":"bevy_light::point_light::PointLight"}]}',
    );
    await tester.pump();

    await tester.tap(find.text('Add component'));
    await tester.pumpAndSettle();
    // The cube has a Transform already.
    expect(find.widgetWithText(ListTile, 'Transform'), findsNothing);
    await tester.enterText(find.byType(TextField).last, 'shape');
    await tester.pump();
    expect(find.text('PointLight'), findsNothing);
    await tester.tap(find.text('MeshShape'));
    await tester.pumpAndSettle();
    expect(game.commands.last, {
      'command': 'insert',
      'id': _cube,
      'component': 'besfa_editor_plugin::scene::MeshShape',
    });

    await tester.tap(find.byTooltip('Remove component'));
    expect(game.commands.last, {
      'command': 'remove',
      'id': _cube,
      'component': _transform,
    });
  });

  testWidgets('Play saves unsaved changes first; leaving asks', (
    WidgetTester tester,
  ) async {
    final (game, _) = await openWithCube(tester);
    await tester.tap(find.byTooltip('Add entity'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Empty'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('File'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Back to Project Hub'));
    await tester.pumpAndSettle();
    expect(find.text('Discard unsaved changes?'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.text('Discard unsaved changes?'), findsNothing);

    await tester.tap(find.text('Play'));
    await tester.pump();
    expect(game.commands.skip(game.commands.length - 2), [
      {'command': 'save'},
      {'command': 'play'},
    ]);
  });

  test('numbers are shown with up to three decimals', () {
    expect(formatNumber(0.699999988079071), '0.7');
    expect(formatNumber(-0.0000001), '0');
    expect(formatNumber(10.0), '10');
    expect(formatNumber(3), '3');
  });
}
