class Project {
  const Project(this.path);

  final String path;

  String get name => path
      .split(RegExp(r'[\\/]'))
      .lastWhere((part) => part.isNotEmpty, orElse: () => path);
}
