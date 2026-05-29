import 'package:besfa_editor/pages/editor/editor_page.dart';
import 'package:flutter/material.dart';

class BesfaApp extends StatelessWidget {
  const BesfaApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Besfa',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF148F77)),
        useMaterial3: true,
      ),
      home: const EditorPage(),
    );
  }
}
