import 'package:flutter/widgets.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/app/window.dart';

void main() async {
  await initializeEditorWindow();
  runApp(const BesfaEditorApp());
}
