import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/app/app.dart';

void main() {
  testWidgets('shows the project hub', (WidgetTester tester) async {
    await tester.pumpWidget(const BesfaEditorApp());

    expect(find.text('Start creating'), findsOneWidget);
    expect(find.text('Create project'), findsOneWidget);
    expect(find.text('Open project'), findsOneWidget);
  });
}
