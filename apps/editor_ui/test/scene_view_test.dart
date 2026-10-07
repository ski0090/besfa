import 'dart:io';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';
import 'package:editor_ui/widgets/scene_viewport/lib/bevy_key.dart';
import 'package:editor_ui/widgets/scene_viewport/ui/scene_viewport.dart';

import 'support/editor_harness.dart';

void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('besfa_view'));
  tearDown(() => dir.deleteSync(recursive: true));

  testWidgets('the viewport sends the pointer in texture pixels', (
    WidgetTester tester,
  ) async {
    final commands = <Map<String, Object?>>[];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Align(
          alignment: Alignment.topLeft,
          child: SizedBox(
            width: 640,
            height: 360,
            child: SceneViewport(
              textureId: 0,
              textureSize: const Size(1280, 720),
              onCommand: commands.add,
            ),
          ),
        ),
      ),
    );

    final mouse = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await mouse.down(const Offset(100, 50));
    await mouse.moveTo(const Offset(110, 60));
    await mouse.up();
    await mouse.removePointer();
    expect(commands, [
      {
        'command': 'pointer',
        'event': 'down',
        'x': 200.0,
        'y': 100.0,
        'button': 'right',
      },
      {'command': 'pointer', 'event': 'move', 'x': 220.0, 'y': 120.0},
      {
        'command': 'pointer',
        'event': 'up',
        'x': 220.0,
        'y': 120.0,
        'button': 'right',
      },
    ]);

    commands.clear();
    final wheel = TestPointer(2, PointerDeviceKind.mouse);
    await tester.sendEventToBinding(wheel.hover(const Offset(320, 180)));
    await tester.sendEventToBinding(wheel.scroll(const Offset(0, 120)));
    expect(commands.last, {
      'command': 'pointer',
      'event': 'scroll',
      'delta': 120.0,
    });
  });

  test('keys are named the way Bevy names its key codes', () {
    expect(bevyKeyCode(PhysicalKeyboardKey.keyW), 'KeyW');
    expect(bevyKeyCode(PhysicalKeyboardKey.keyZ), 'KeyZ');
    expect(bevyKeyCode(PhysicalKeyboardKey.digit1), 'Digit1');
    expect(bevyKeyCode(PhysicalKeyboardKey.digit0), 'Digit0');
    expect(bevyKeyCode(PhysicalKeyboardKey.f12), 'F12');
    expect(bevyKeyCode(PhysicalKeyboardKey.metaLeft), 'SuperLeft');
    expect(bevyKeyCode(PhysicalKeyboardKey.arrowUp), 'ArrowUp');
    expect(bevyKeyCode(PhysicalKeyboardKey.fn), isNull);
  });

  testWidgets('a playing game gets the keys while the viewport has them', (
    WidgetTester tester,
  ) async {
    final commands = <Map<String, Object?>>[];
    Future<void> show({required bool playing}) => tester.pumpWidget(
      MaterialApp(
        home: Column(
          children: [
            SizedBox(
              width: 640,
              height: 360,
              child: SceneViewport(
                textureId: 0,
                textureSize: const Size(640, 360),
                onCommand: commands.add,
                playing: playing,
              ),
            ),
            const Text('Elsewhere'),
          ],
        ),
      ),
    );
    List<Map<String, Object?>> keys() => [
      for (final command in commands)
        if (command['command'] != 'pointer') command,
    ];

    await show(playing: false);
    await tester.tapAt(const Offset(100, 100));
    await tester.sendKeyEvent(LogicalKeyboardKey.keyW);
    expect(keys(), isEmpty, reason: 'the edit mode keeps its shortcuts');

    // Play hands the keyboard to the viewport.
    await tester.tap(find.text('Elsewhere'));
    await show(playing: true);
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyW);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyW);
    expect(keys(), [
      {'command': 'key', 'code': 'KeyW', 'pressed': true, 'text': 'w'},
      {'command': 'key', 'code': 'KeyW', 'pressed': false},
    ]);

    // Clicking elsewhere takes the keyboard back and lets held keys go.
    commands.clear();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.space);
    await tester.tap(find.text('Elsewhere'));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.keyA);
    expect(keys(), [
      {'command': 'key', 'code': 'Space', 'pressed': true, 'text': ' '},
      {'command': 'focus_lost'},
    ]);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.space);

    // Clicking the viewport gives it back.
    commands.clear();
    await tester.tapAt(const Offset(100, 100));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.arrowUp);
    expect(keys(), [
      {'command': 'key', 'code': 'ArrowUp', 'pressed': true},
    ]);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.arrowUp);
  });

  testWidgets('holding the right button, WASD, Q and E fly the scene view', (
    WidgetTester tester,
  ) async {
    final commands = <Map<String, Object?>>[];
    final looking = <bool>[];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SceneViewport(
          textureId: 0,
          textureSize: const Size(800, 600),
          onCommand: commands.add,
          onLooking: looking.add,
        ),
      ),
    );
    List<Map<String, Object?>> flights() => [
      for (final command in commands)
        if (command['command'] == 'fly') command,
    ];
    Map<String, Object?> fly(int right, int up, int forward, bool fast) => {
      'command': 'fly',
      'right': right,
      'up': up,
      'forward': forward,
      'fast': fast,
    };

    // Without the right button, keys are the editor's shortcuts.
    await tester.tapAt(const Offset(100, 100));
    await tester.sendKeyEvent(LogicalKeyboardKey.keyW);
    expect(flights(), isEmpty);

    final mouse = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await mouse.down(const Offset(100, 100));
    await tester.pump();
    expect(looking, [true]);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyW);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyE);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyW);
    expect(flights(), [
      fly(0, 0, 1, false),
      fly(0, 1, 1, false),
      fly(0, 1, 1, true),
      fly(0, 1, 0, true),
    ]);
    await mouse.up();
    await mouse.removePointer();
    expect(looking, [true, false]);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyE);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    expect(flights(), hasLength(4), reason: 'released keys fly nothing');
  });

  testWidgets('while playing, the editor leaves Delete to the game', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openEditor(tester, dir);
    output(
      '@besfa {"type":"entities","entities":['
      '{"id":7,"name":"Cube","parent":null,"scene":true}]}',
    );
    await tester.pump();
    await tester.tap(find.text('Cube'));
    await tester.tap(find.text('Play'));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    expect(game.commands.where((c) => c['command'] == 'delete'), isEmpty);
  });

  testWidgets('W, E and R pick the tool; F frames; drags mark unsaved', (
    WidgetTester tester,
  ) async {
    final (game, output) = await openEditor(tester, dir);

    await tester.sendKeyEvent(LogicalKeyboardKey.keyE);
    await tester.pump();
    expect(game.commands.last, {'command': 'tool', 'tool': 'rotate'});
    expect(
      tester
          .widget<IconButton>(
            find.widgetWithIcon(IconButton, Icons.rotate_right),
          )
          .isSelected,
      isTrue,
    );
    await tester.tap(find.byTooltip('Scale (R)'));
    expect(game.commands.last, {'command': 'tool', 'tool': 'scale'});
    await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
    expect(game.commands.last, {'command': 'focus'});

    output(
      '@besfa {"type":"edited","id":1,"component":"t::T",'
      '"before":{"x":0},"after":{"x":1}}',
    );
    await tester.pump();
    expect(find.textContaining(' *'), findsOneWidget);
  });

  testWidgets(
    'the viewport texture follows the panel and the game follows it',
    (WidgetTester tester) async {
      tester.view.physicalSize = const Size(1600, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final messenger = tester.binding.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(
        const MethodChannel('window_manager'),
        (call) async => call.method == 'isMaximized' ? false : null,
      );
      final created = <Map<Object?, Object?>>[];
      final disposed = <Object?>[];
      messenger.setMockMethodCallHandler(
        const MethodChannel('besfa/viewport'),
        (call) async {
          if (call.method == 'dispose') {
            disposed.add(call.arguments);
            return null;
          }
          created.add(call.arguments as Map<Object?, Object?>);
          return {
            'textureId': created.length,
            'name': 'viewport-${created.length}',
            'adapter': 'gpu',
          };
        },
      );
      final games = <FakeGame>[];
      await tester.pumpWidget(
        MaterialApp(
          home: ProjectEditorPage(
            project: Project(dir.path),
            startGame: (_, {required environment, required onOutput}) async {
              games.add(FakeGame());
              return games.last;
            },
          ),
        ),
      );
      await tester.pump();
      expect(created.single, {'width': 1280, 'height': 720});

      // The panel's size, once it has settled.
      await tester.pump(const Duration(milliseconds: 300));
      await tester.pump();
      expect(created, hasLength(2));
      final panel = created.last;
      expect(panel['width'], isNot(1280));
      expect(games.single.commands.last, {
        'command': 'viewport',
        'name': 'viewport-2',
      });
      final texture = tester.widget<Texture>(find.byType(Texture));
      expect(texture.textureId, 2);
      expect(disposed, isEmpty, reason: 'the game may still render into it');

      // E while the right button is held flies up instead of picking the
      // rotate tool; once it is up, E picks the tool again.
      final game = games.single;
      final mouse = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await mouse.down(tester.getCenter(find.byType(Texture)));
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.keyE);
      expect(game.commands.where((c) => c['command'] == 'tool'), isEmpty);
      expect(game.commands.where((c) => c['command'] == 'fly'), isNotEmpty);
      await mouse.up();
      await mouse.removePointer();
      await tester.sendKeyEvent(LogicalKeyboardKey.keyE);
      expect(game.commands.last, {'command': 'tool', 'tool': 'rotate'});
      await tester.pumpAndSettle();

      // Stop relaunches: the old game is gone, and so is its texture.
      await tester.tap(find.text('Play'));
      await tester.pump();
      await tester.tap(find.text('Stop'));
      await tester.pumpAndSettle();
      expect(games, hasLength(2));
      expect(disposed, [1]);
    },
  );
}
