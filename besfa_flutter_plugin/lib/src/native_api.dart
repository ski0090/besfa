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

@Native<Int32 Function()>(symbol: 'besfa_runtime_start')
external int besfaRuntimeStart();

@Native<Int32 Function()>(symbol: 'besfa_runtime_stop')
external int besfaRuntimeStop();

@Native<Int32 Function()>(symbol: 'besfa_runtime_status')
external int besfaRuntimeStatus();

@Native<Int32 Function()>(symbol: 'besfa_runtime_last_error_code')
external int besfaRuntimeLastErrorCode();
