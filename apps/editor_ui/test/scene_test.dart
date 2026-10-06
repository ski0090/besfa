import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';

void main() {
  test('reads the entity, component and system reports', () {
    final scene = Scene();
    var notified = 0;
    scene.addListener(() => notified++);

    expect(
      scene.handle(
        '@besfa {"type":"entities","entities":['
        '{"id":4294967295,"name":"Cube","parent":null},'
        '{"id":4294967294,"name":null,"parent":4294967295}]}',
      ),
      isTrue,
    );
    expect(scene.entities.map((e) => e.label), ['Cube', 'Entity 1v0']);
    expect(scene.entities.last.parent, 4294967295);

    scene.select(4294967295);
    expect(
      scene.handle(
        '@besfa {"type":"entity","id":4294967295,"components":['
        '{"name":"Transform","path":"bevy_transform::components::transform::Transform",'
        '"mutable":true,"required_by":null,"value":{"translation":[1.0,2.0,3.0]}},'
        '{"name":"Spin","path":"my_game::Spin","mutable":true,"required_by":null,"value":null}]}',
      ),
      isTrue,
    );
    expect(scene.components.map((c) => c.crate), ['bevy_transform', 'my_game']);
    expect(scene.components.first.value, {
      'translation': [1.0, 2.0, 3.0],
    });
    expect(scene.components.last.value, isNull);

    expect(
      scene.handle(
        '@besfa {"type":"systems","crate":"my_game","systems":['
        '{"schedule":"Update","name":"my_game::spin"},'
        '{"schedule":"PostUpdate","name":"<bevy_a::B as C>::run"}]}',
      ),
      isTrue,
    );
    expect(scene.crate, 'my_game');
    expect(scene.systems.map((s) => s.crate), ['my_game', 'bevy_a']);
    expect(notified, 4);
  });

  test('ignores reports for an entity that is no longer selected', () {
    final scene = Scene()..select(1);

    scene.handle('@besfa {"type":"entity","id":2,"components":[]}');
    expect(scene.components, isEmpty);

    scene.handle(
      '@besfa {"type":"entity","id":1,"components":[{"name":"A","path":"a::A","mutable":true}]}',
    );
    expect(scene.components.single.name, 'A');

    scene.select(null);
    expect(scene.components, isEmpty);
  });

  test('leaves logs and malformed reports to the log panel', () {
    final scene = Scene();

    expect(scene.handle('INFO my_game: Playing.'), isFalse);
    expect(scene.handle('@besfa not json'), isFalse);
    expect(scene.handle('@besfa {"type":"entities","entities":"?"}'), isFalse);
  });

  test('reset keeps the selection for the next game process', () {
    final scene = Scene()..select(7);
    scene.handle('@besfa {"type":"entities","entities":[{"id":7}]}');

    scene.reset();

    expect(scene.entities, isEmpty);
    expect(scene.selected, 7);
  });
}
