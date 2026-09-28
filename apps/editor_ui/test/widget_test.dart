import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/features/create_project/model/project_creator.dart';

void main() {
  testWidgets('shows the project hub', (WidgetTester tester) async {
    await tester.pumpWidget(const BesfaEditorApp());

    expect(find.text('Start creating'), findsOneWidget);
    expect(find.text('Create project'), findsOneWidget);
    expect(find.text('Open project'), findsOneWidget);
  });

  testWidgets('creates a project and opens it in the editor', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      BesfaEditorApp(projectCreator: _FakeProjectCreator()),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'New project'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), r'C:\Projects\demo_game');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    expect(find.text('Viewport'), findsOneWidget);

    await tester.tap(find.text('Close project'));
    await tester.pumpAndSettle();
    expect(find.text('Start creating'), findsOneWidget);
  });
}

class _FakeProjectCreator implements ProjectCreator {
  @override
  Future<ProjectCreationResult> create(String directory) async {
    return ProjectCreationSuccess(directory);
  }
}
