import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';
import 'package:editor_ui/shared/process/cli_process.dart';

/// The window manager's calls; tests have no window.
final windowCalls = <String>[];

/// Stands in for the window manager and the native viewport, which tests
/// do not have.
void mockEditorChannels(WidgetTester tester) {
  final messenger = tester.binding.defaultBinaryMessenger;
  windowCalls.clear();
  messenger.setMockMethodCallHandler(const MethodChannel('window_manager'), (
    call,
  ) async {
    windowCalls.add(call.method);
    return call.method == 'isMaximized' ? false : null;
  });
  messenger.setMockMethodCallHandler(
    const MethodChannel('besfa/viewport'),
    (call) async => throw PlatformException(code: 'unavailable'),
  );
}

/// Clicks the window's close button, the way the window manager tells it.
Future<void> closeWindow(WidgetTester tester) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    'window_manager',
    const StandardMethodCodec().encodeMethodCall(
      const MethodCall('onEvent', {'eventName': 'close'}),
    ),
    (_) {},
  );
  await tester.pumpAndSettle();
}

/// A game that runs until stopped and records what the editor sends it.
class FakeGame implements CliProcess {
  final sent = <String>[];
  final _exit = Completer<int>();

  /// What the editor sent, decoded: one JSON command per line.
  List<Map<String, Object?>> get commands => [
    for (final line in sent) jsonDecode(line) as Map<String, Object?>,
  ];

  @override
  Future<int> get exitCode => _exit.future;

  @override
  void send(String line) => sent.add(line);

  /// Exits the way a killed process does.
  @override
  Future<void> stop() async {
    if (!_exit.isCompleted) {
      _exit.complete(1);
    }
  }
}

/// The editor page on a project in [directory] with a [FakeGame], on a
/// screen large enough for every panel. Returns the game and a way to
/// write lines to the editor as the game.
Future<(FakeGame, void Function(String line))> openEditor(
  WidgetTester tester,
  Directory directory,
) async {
  mockEditorChannels(tester);
  tester.view.physicalSize = const Size(1600, 1100);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final game = FakeGame();
  late void Function(String line) output;
  await tester.pumpWidget(
    MaterialApp(
      home: ProjectEditorPage(
        project: Project(directory.path),
        startGame: (_, {required environment, required onOutput}) async {
          output = onOutput;
          return game;
        },
      ),
    ),
  );
  await tester.pump();
  return (game, (String line) => output(line));
}
