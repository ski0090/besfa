import 'dart:math' as math;

/// A quaternion `[x, y, z, w]` as Euler angles in degrees `[x, y, z]`:
/// Bevy's `EulerRot::YXZ`, yaw about Y, then pitch about X, then roll
/// about Z, the order Bevy's own examples use.
List<double> quatToEuler(List<num> quat) {
  final [x, y, z, w] = [for (final value in quat) value.toDouble()];
  // The rotation matrix entries the angles come from.
  final m02 = 2 * (x * z + w * y);
  final m22 = 1 - 2 * (x * x + y * y);
  final m12 = 2 * (y * z - w * x);
  final m10 = 2 * (x * y + w * z);
  final m11 = 1 - 2 * (x * x + z * z);
  return [
    math.asin((-m12).clamp(-1.0, 1.0)),
    math.atan2(m02, m22),
    math.atan2(m10, m11),
  ].map((radians) => radians * 180 / math.pi).toList();
}

/// The quaternion `[x, y, z, w]` of [quatToEuler]'s angles.
List<double> eulerToQuat(List<num> degrees) {
  final [pitch, yaw, roll] = [
    for (final value in degrees) value * math.pi / 180,
  ];
  return _multiply(
    _multiply(
      [0, math.sin(yaw / 2), 0, math.cos(yaw / 2)],
      [math.sin(pitch / 2), 0, 0, math.cos(pitch / 2)],
    ),
    [0, 0, math.sin(roll / 2), math.cos(roll / 2)],
  );
}

List<double> _multiply(List<double> a, List<double> b) {
  final [ax, ay, az, aw] = a;
  final [bx, by, bz, bw] = b;
  return [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ];
}
