import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:editor_ui/shared/ui/panel.dart';
import 'package:editor_ui/widgets/entity_inspector/lib/rotation.dart';

/// Edits a component's reflected JSON value: numbers, text and flags become
/// fields, nested structs indent under their field name. [onChanged] gets
/// the whole new value, since the game replaces the component with it.
class ValueEditor extends StatelessWidget {
  const ValueEditor({
    super.key,
    required this.value,
    required this.onChanged,
    this.eulerRotation = false,
  });

  final Object? value;
  final ValueChanged<Object?> onChanged;

  /// Shows the `rotation` quaternion as Euler angles in degrees.
  final bool eulerRotation;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: _fields(value, const [], null),
    );
  }

  List<Widget> _fields(Object? node, List<Object> path, String? label) {
    if (node is Map<String, Object?>) {
      final children = [
        for (final MapEntry(:key, :value) in node.entries)
          ..._fields(value, [...path, key], key),
      ];
      if (label == null) {
        return children;
      }
      return [
        _row(label, null),
        Padding(
          padding: const EdgeInsets.only(left: 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: children,
          ),
        ),
      ];
    }
    return [_row(label, _leaf(node, path))];
  }

  Widget _leaf(Object? node, List<Object> path) {
    final key = ValueKey(path.join('/'));
    void change(List<Object> at, Object? leaf) =>
        onChanged(_replace(value, at, leaf));
    return switch (node) {
      num() => _NumberField(
        key: key,
        value: node,
        onSubmitted: (number) => change(path, number),
      ),
      bool() => Align(
        alignment: Alignment.centerLeft,
        child: Checkbox(
          key: key,
          value: node,
          visualDensity: VisualDensity.compact,
          materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
          onChanged: (checked) => change(path, checked),
        ),
      ),
      String() => _TextField(
        key: key,
        value: node,
        onSubmitted: (text) => change(path, text),
      ),
      List() when _isVector(node) =>
        eulerRotation && path.length == 1 && path.single == 'rotation'
            ? _vector(key, quatToEuler(node.cast()), const [
                'X',
                'Y',
                'Z',
              ], (angles) => change(path, eulerToQuat(angles)))
            : _vector(key, node.cast(), const [
                'x',
                'y',
                'z',
                'w',
              ], (vector) => change(path, vector)),
      null => const _ReadOnly('None'),
      _ => _ReadOnly(jsonEncode(node)),
    };
  }

  /// One field per component; [onChanged] gets the whole new vector.
  Widget _vector(
    Key key,
    List<num> vector,
    List<String> labels,
    ValueChanged<List<num>> onChanged,
  ) {
    return Row(
      key: key,
      children: [
        for (var i = 0; i < vector.length; i++) ...[
          if (i > 0) const SizedBox(width: 4),
          Expanded(
            child: _NumberField(
              value: vector[i],
              label: labels[i],
              onSubmitted: (number) =>
                  onChanged([...vector]..[i] = number.toDouble()),
            ),
          ),
        ],
      ],
    );
  }
}

bool _isVector(List<Object?> list) =>
    list.length >= 2 && list.length <= 4 && list.every((item) => item is num);

/// A copy of [node] with the value at [path] replaced by [leaf].
Object? _replace(Object? node, List<Object> path, Object? leaf) {
  if (path.isEmpty) {
    return leaf;
  }
  final [head, ...rest] = path;
  return switch ((node, head)) {
    (Map<String, Object?> map, String key) => {
      ...map,
      key: _replace(map[key], rest, leaf),
    },
    (List<Object?> list, int index) => [
      ...list,
    ]..[index] = _replace(list[index], rest, leaf),
    _ => leaf,
  };
}

Widget _row(String? label, Widget? child) {
  if (label == null) {
    return child ?? const SizedBox.shrink();
  }
  return Padding(
    padding: const EdgeInsets.symmetric(vertical: 2),
    child: Row(
      children: [
        SizedBox(
          width: 84,
          child: Text(
            label,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 11, color: panelMutedText),
          ),
        ),
        if (child != null) Expanded(child: child),
      ],
    ),
  );
}

/// Three decimals, without trailing zeros: what fits in a field.
String formatNumber(num value) {
  if (value is int) {
    return '$value';
  }
  final text = value.toStringAsFixed(3).replaceFirst(RegExp(r'\.?0+$'), '');
  return text == '-0' ? '0' : text;
}

InputDecoration _decoration(String? prefix) => InputDecoration(
  isDense: true,
  prefixText: prefix == null ? null : '$prefix ',
  prefixStyle: const TextStyle(fontSize: 11, color: panelMutedText),
  filled: true,
  fillColor: const Color(0xFF111316),
  contentPadding: const EdgeInsets.symmetric(horizontal: 6, vertical: 7),
  border: OutlineInputBorder(
    borderSide: BorderSide.none,
    borderRadius: BorderRadius.circular(3),
  ),
  focusedBorder: OutlineInputBorder(
    borderSide: const BorderSide(color: Color(0xFF8CB4FF)),
    borderRadius: BorderRadius.circular(3),
  ),
);

const _fieldText = TextStyle(
  fontSize: 11,
  fontFamily: 'Consolas',
  color: panelText,
);

/// A text field that commits when it loses focus, Enter included, and
/// takes the game's newest value only while nobody is typing in it.
abstract class _CommitField<T> extends StatefulWidget {
  const _CommitField({
    super.key,
    required this.value,
    required this.onSubmitted,
    this.label,
  });

  final T value;
  final ValueChanged<T> onSubmitted;
  final String? label;

  String format(T value);

  /// The typed text as a value, or null to put the old value back.
  T? parse(String text);

  @override
  State<_CommitField<T>> createState() => _CommitFieldState<T>();
}

class _CommitFieldState<T> extends State<_CommitField<T>> {
  late final _controller = TextEditingController(
    text: widget.format(widget.value),
  );
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _focus.addListener(() {
      if (!_focus.hasFocus) {
        _commit();
      }
    });
  }

  @override
  void didUpdateWidget(_CommitField<T> oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_focus.hasFocus) {
      _controller.text = widget.format(widget.value);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _commit() {
    final value = widget.parse(_controller.text);
    if (value == null) {
      _controller.text = widget.format(widget.value);
    } else if (widget.format(value) != widget.format(widget.value)) {
      widget.onSubmitted(value);
    }
  }

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: _controller,
      focusNode: _focus,
      style: _fieldText,
      decoration: _decoration(widget.label),
      // Enter leaves the field, which commits it.
      onEditingComplete: _focus.unfocus,
    );
  }
}

class _NumberField extends _CommitField<num> {
  const _NumberField({
    super.key,
    required super.value,
    required super.onSubmitted,
    super.label,
  });

  @override
  String format(num value) => formatNumber(value);

  /// An integer field stays an integer; a float takes whatever is typed.
  @override
  num? parse(String text) {
    final number = num.tryParse(text.trim());
    if (number == null || value is! int) {
      return number?.toDouble();
    }
    return number == number.roundToDouble() ? number.round() : null;
  }
}

class _TextField extends _CommitField<String> {
  const _TextField({
    super.key,
    required super.value,
    required super.onSubmitted,
  });

  @override
  String format(String value) => value;

  @override
  String parse(String text) => text;
}

class _ReadOnly extends StatelessWidget {
  const _ReadOnly(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return SelectableText(text, style: _fieldText);
  }
}
