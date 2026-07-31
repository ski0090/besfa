import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

Future<void> initializeEditorWindow() async {
  WidgetsFlutterBinding.ensureInitialized();
  await windowManager.ensureInitialized();

  const windowOptions = WindowOptions(
    title: 'Besfa Editor',
    size: Size(1280, 800),
    minimumSize: Size(900, 640),
    center: true,
  );

  windowManager.waitUntilReadyToShow(windowOptions, () async {
    await windowManager.show();
    await windowManager.focus();
  });
}
