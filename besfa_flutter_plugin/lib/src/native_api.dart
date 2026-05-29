@DefaultAsset('package:besfa_flutter_plugin/src/ffi.g.dart')
library;

import 'dart:ffi';

@Native<Uint32 Function()>(
  symbol: 'besfa_flutter_plugin_abi_version',
  isLeaf: true,
)
external int besfaFlutterPluginAbiVersion();

@Native<Int32 Function(Int32, Int32)>(
  symbol: 'besfa_flutter_plugin_add',
  isLeaf: true,
)
external int besfaFlutterPluginAdd(int left, int right);
