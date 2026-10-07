import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/widgets/entity_inspector/lib/rotation.dart';

void main() {
  test('reads Bevy rotations as yaw, pitch and roll in degrees', () {
    // Quat::from_rotation_y(90°) and Quat::from_rotation_x(90°).
    expect(quatToEuler([0, 0.7071068, 0, 0.7071068]), [
      closeTo(0, 1e-4),
      closeTo(90, 1e-4),
      closeTo(0, 1e-4),
    ]);
    expect(quatToEuler([0.7071068, 0, 0, 0.7071068])[0], closeTo(90, 1e-3));
    expect(quatToEuler([0, 0, 0, 1]), [0, 0, 0]);
  });

  test('turns angles back into the same rotation', () {
    for (final angles in [
      [10.0, 20.0, 30.0],
      [-45.0, 170.0, -5.0],
      [0.0, -90.0, 0.0],
    ]) {
      final back = quatToEuler(eulerToQuat(angles));
      for (var i = 0; i < 3; i++) {
        expect(back[i], closeTo(angles[i], 1e-6), reason: '$angles');
      }
    }
    // The template camera's looking_at rotation survives a round trip.
    const camera = [-0.22055435, -0.13167094, -0.030063398, 0.9659786];
    final quat = eulerToQuat(quatToEuler(camera));
    for (var i = 0; i < 4; i++) {
      expect(quat[i], closeTo(camera[i], 1e-6));
    }
  });
}
