import 'package:flutter/material.dart';

import 'package:editor_ui/pages/project_hub/ui/project_hub_page.dart';

class BesfaEditorApp extends StatelessWidget {
  const BesfaEditorApp({super.key});

  @override
  Widget build(BuildContext context) {
    const colorScheme = ColorScheme.dark(
      primary: Color(0xFF8CB4FF),
      onPrimary: Color(0xFF10264C),
      surface: Color(0xFF20242B),
      onSurface: Color(0xFFE4E7EC),
    );

    return MaterialApp(
      title: 'Besfa',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        brightness: Brightness.dark,
        colorScheme: colorScheme,
        scaffoldBackgroundColor: const Color(0xFF15171B),
        cardTheme: const CardThemeData(
          margin: EdgeInsets.zero,
          elevation: 0,
          color: Color(0xFF20242B),
        ),
      ),
      home: const ProjectHubPage(),
    );
  }
}
