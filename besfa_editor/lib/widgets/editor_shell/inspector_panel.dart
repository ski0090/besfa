import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/shared/ui/panel_title.dart';
import 'package:flutter/material.dart';

/// Inspector panel for the currently selected runtime scene entity.
class InspectorPanel extends StatefulWidget {
  const InspectorPanel({
    required this.selectedEntity,
    required this.cameraPreviewTextureId,
    required this.onSetTranslation,
    required this.onAlignSelectedCameraToEditor,
    super.key,
  });

  /// Entity selected in the runtime scene snapshot, if any.
  final RuntimeSceneEntity? selectedEntity;

  /// Flutter texture id for the selected camera preview, when available.
  final int? cameraPreviewTextureId;

  /// Applies a new translation to the selected runtime entity.
  final ValueChanged<RuntimeVector3> onSetTranslation;

  /// Copies the current editor Scene View camera transform to the selected camera.
  final VoidCallback onAlignSelectedCameraToEditor;

  @override
  State<InspectorPanel> createState() => _InspectorPanelState();
}

class _InspectorPanelState extends State<InspectorPanel> {
  final TextEditingController _x = TextEditingController();
  final TextEditingController _y = TextEditingController();
  final TextEditingController _z = TextEditingController();
  String? _loadedEntityId;
  RuntimeVector3? _loadedTranslation;
  bool _isDirty = false;

  @override
  void initState() {
    super.initState();
    _syncControllers(force: true);
  }

  @override
  void didUpdateWidget(covariant InspectorPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncControllers();
  }

  @override
  void dispose() {
    _x.dispose();
    _y.dispose();
    _z.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final selectedEntity = widget.selectedEntity;
    final translation = selectedEntity?.transform?.translation;

    return ListView(
      padding: const EdgeInsets.all(12),
      children: [
        const PanelTitle('Inspector'),
        if (selectedEntity == null)
          Text(
            'No selection',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          )
        else ...[
          _PropertyRow(label: 'Name', value: selectedEntity.name),
          _PropertyRow(label: 'Kind', value: selectedEntity.kind),
          _PropertyRow(label: 'Entity ID', value: selectedEntity.id),
          const SizedBox(height: 12),
          Text('Position', style: Theme.of(context).textTheme.labelLarge),
          const SizedBox(height: 8),
          if (translation == null)
            Text(
              'No transform',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            )
          else
            _PositionEditor(
              x: _x,
              y: _y,
              z: _z,
              canApply: _isDirty && _parsedTranslation() != null,
              onChanged: _markDirty,
              onApply: _applyTranslation,
            ),
          if (selectedEntity.kind == 'camera') ...[
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                  child: Text(
                    'Camera Preview',
                    style: Theme.of(context).textTheme.labelLarge,
                  ),
                ),
                SizedBox(
                  width: 36,
                  height: 36,
                  child: IconButton(
                    tooltip: 'Align to Scene View',
                    onPressed: widget.onAlignSelectedCameraToEditor,
                    icon: const Icon(Icons.center_focus_strong),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            _CameraPreview(textureId: widget.cameraPreviewTextureId),
          ],
        ],
      ],
    );
  }

  void _syncControllers({bool force = false}) {
    final entity = widget.selectedEntity;
    final translation = entity?.transform?.translation;
    final shouldSync =
        force ||
        entity?.id != _loadedEntityId ||
        !_sameTranslation(translation, _loadedTranslation);

    if (!shouldSync) {
      return;
    }

    _loadedEntityId = entity?.id;
    _loadedTranslation = translation;
    _x.text = _formatComponent(translation?.x);
    _y.text = _formatComponent(translation?.y);
    _z.text = _formatComponent(translation?.z);
    _isDirty = false;
  }

  void _markDirty() {
    setState(() {
      _isDirty = true;
    });
  }

  void _applyTranslation() {
    final translation = _parsedTranslation();
    if (translation == null) {
      return;
    }

    widget.onSetTranslation(translation);
    setState(() {
      _isDirty = false;
    });
  }

  RuntimeVector3? _parsedTranslation() {
    final x = double.tryParse(_x.text.trim());
    final y = double.tryParse(_y.text.trim());
    final z = double.tryParse(_z.text.trim());
    if (x == null || y == null || z == null) {
      return null;
    }

    return RuntimeVector3(x: x, y: y, z: z);
  }

  bool _sameTranslation(RuntimeVector3? left, RuntimeVector3? right) {
    return left?.x == right?.x && left?.y == right?.y && left?.z == right?.z;
  }

  String _formatComponent(double? value) {
    if (value == null) {
      return '';
    }

    return value.toStringAsFixed(2);
  }
}

class _CameraPreview extends StatelessWidget {
  const _CameraPreview({required this.textureId});

  final int? textureId;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: 16 / 9,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0xFF101314),
          border: Border.all(color: const Color(0xFF303637)),
        ),
        child: textureId == null
            ? Center(
                child: Text(
                  'Waiting for camera preview',
                  style: Theme.of(
                    context,
                  ).textTheme.bodySmall?.copyWith(color: Colors.white70),
                ),
              )
            : Texture(textureId: textureId!),
      ),
    );
  }
}

class _PositionEditor extends StatelessWidget {
  const _PositionEditor({
    required this.x,
    required this.y,
    required this.z,
    required this.canApply,
    required this.onChanged,
    required this.onApply,
  });

  final TextEditingController x;
  final TextEditingController y;
  final TextEditingController z;
  final bool canApply;
  final VoidCallback onChanged;
  final VoidCallback onApply;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: _AxisField(label: 'X', controller: x, onChanged: onChanged),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: _AxisField(label: 'Y', controller: y, onChanged: onChanged),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: _AxisField(label: 'Z', controller: z, onChanged: onChanged),
        ),
        const SizedBox(width: 4),
        SizedBox(
          width: 40,
          height: 40,
          child: IconButton(
            tooltip: 'Apply position',
            onPressed: canApply ? onApply : null,
            icon: const Icon(Icons.check),
          ),
        ),
      ],
    );
  }
}

class _AxisField extends StatelessWidget {
  const _AxisField({
    required this.label,
    required this.controller,
    required this.onChanged,
  });

  final String label;
  final TextEditingController controller;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      keyboardType: const TextInputType.numberWithOptions(
        decimal: true,
        signed: true,
      ),
      decoration: InputDecoration(
        labelText: label,
        border: const OutlineInputBorder(),
        contentPadding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
      ),
      onChanged: (_) => onChanged(),
      onSubmitted: (_) => onChanged(),
    );
  }
}

class _PropertyRow extends StatelessWidget {
  const _PropertyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 92,
            child: Text(label, style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(child: SelectableText(value)),
        ],
      ),
    );
  }
}
