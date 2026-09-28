import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/features/create_project/model/project_creator.dart';
import 'package:editor_ui/pages/project_hub/ui/project_hub_page.dart';

void main() {
  testWidgets('shows the project hub', (WidgetTester tester) async {
    await tester.pumpWidget(const BesfaEditorApp());

    expect(find.text('Start creating'), findsOneWidget);
    expect(find.text('Create project'), findsOneWidget);
    expect(find.text('Open project'), findsOneWidget);
  });

  testWidgets('creates a project through the CLI feature', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: ProjectHubPage(projectCreator: _FakeProjectCreator())),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'New project'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), r'C:\Projects\demo_game');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    expect(find.text("Created 'C:\\Projects\\demo_game'."), findsOneWidget);
  });
}

class _FakeProjectCreator implements ProjectCreator {
  @override
  Future<ProjectCreationResult> create(String directory) async {
    return ProjectCreationSuccess(directory);
  }
}
