import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

import 'package:editor_ui/entities/project/model/project.dart';

class ProjectEditorPage extends StatefulWidget {
  const ProjectEditorPage({super.key, required this.project});

  final Project project;

  @override
  State<ProjectEditorPage> createState() => _ProjectEditorPageState();
}

const _captionColor = Color(0xFF15171B);

class _ProjectEditorPageState extends State<ProjectEditorPage> {
  @override
  void initState() {
    super.initState();
    windowManager.setTitleBarStyle(
      TitleBarStyle.hidden,
      windowButtonVisibility: false,
    );
  }

  @override
  void dispose() {
    windowManager.setTitleBarStyle(TitleBarStyle.normal);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: PreferredSize(
        preferredSize: const Size.fromHeight(kWindowCaptionHeight),
        child: ColoredBox(
          color: _captionColor,
          child: Stack(
            children: [
              Row(
                children: [
                  // Kept outside WindowCaption: its drag area waits for a
                  // double tap, which would delay every button press inside it.
                  MenuBar(
                    style: const MenuStyle(
                      backgroundColor: WidgetStatePropertyAll(_captionColor),
                      elevation: WidgetStatePropertyAll(0),
                    ),
                    children: [
                      SubmenuButton(
                        menuChildren: [
                          MenuItemButton(
                            leadingIcon: const Icon(Icons.arrow_back, size: 16),
                            onPressed: () => Navigator.of(context).pop(),
                            child: const Text('Back to Project Hub'),
                          ),
                        ],
                        child: const Text('File'),
                      ),
                    ],
                  ),
                  Expanded(
                    child: WindowCaption(
                      brightness: Theme.of(context).brightness,
                      backgroundColor: _captionColor,
                    ),
                  ),
                ],
              ),
              // Centered on the whole bar; ignores pointers so dragging it
              // still moves the window.
              IgnorePointer(
                child: Center(child: _ProjectTitle(widget.project.name)),
              ),
            ],
          ),
        ),
      ),
      body: const Center(
        child: Text('Viewport', style: TextStyle(color: Color(0xFF7E8795))),
      ),
    );
  }
}

class _ProjectTitle extends StatelessWidget {
  const _ProjectTitle(this.name);

  final String name;

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: const BoxConstraints(minWidth: 240, maxWidth: 480),
      height: 22,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: Colors.white.withValues(alpha: .04),
        border: Border.all(color: Colors.white.withValues(alpha: .14)),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        name,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontSize: 12, color: Color(0xFFB3BBC8)),
      ),
    );
  }
}
