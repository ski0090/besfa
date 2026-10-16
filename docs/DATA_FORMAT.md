# 에디터 데이터 포맷

이 문서는 `besfa_data` crate를 구현하기 전에 확정할 초기 `.besfa` 데이터 포맷의 초안이다. 씬은 예외로, 아래 '씬 파일'대로 Bevy의 월드 형식을 그대로 쓴다.

## 씬 파일

게임의 씬은 프로젝트의 에셋 폴더 안 `assets/scenes/main.scn.ron`에 Bevy의 월드 형식(`bevy_world_serialization`의 RON)으로 저장한다. 게임의 `besfa_editor_plugin`이 시작할 때 이 파일을 읽어 엔티티를 만들고(`SceneEntity` 표식을 붙인다), 에디터의 File > Save scene은 그 엔티티들을 같은 파일에 다시 쓴다. `besfa new`가 큐브·조명·카메라가 든 파일을 만들고, 사람이 직접 고쳐도 된다.

- 엔티티마다 `Reflect`로 등록된 컴포넌트만 들어간다. 게임 자신의 컴포넌트는 `my_game::Spin`처럼 크레이트 이름이 앞에 붙는다.
- 쓰지 않은 필드는 기본값을 쓴다 (`#[reflect(Default)]` 타입).
- 메시와 머티리얼은 핸들이라 파일에 넣을 수 없으므로, 플러그인의 `MeshShape`(상자·구·평면)와 `MeshColor`를 넣고 플러그인이 메시와 머티리얼을 만든다. 둘이 바뀌면 다시 만들고, 떼어 내면 메시나 머티리얼도 뗀다. 둘 다 기본값이 있어 에디터에서 붙일 수 있다.
- 저장할 때는 핸들, `GlobalTransform`·가시성·`Aabb`·`Frustum`처럼 다른 컴포넌트에서 계산되는 것, 직렬화되지 않는 컴포넌트를 뺀다 (직렬화되지 않는 것은 경고를 남긴다). 다른 저장되는 컴포넌트가 요구해서 붙은 컴포넌트(`Camera3d`의 `Camera`, `Projection` 등)는 값이 그 타입의 기본값이면 쓰지 않는다. 불러올 때 요구한 컴포넌트가 다시 붙이기 때문이다. 카메라의 행렬·대상 크기(`Camera`의 `computed`)와 원근 투영의 화면비는 Bevy가 매 프레임 다시 계산하므로 기본값으로 쓴다. 그래서 템플릿 카메라처럼 따로 고치지 않은 `Camera`와 `Projection`은 파일에 들어가지 않는다. 요구한 쪽이 기본값이 아닌 값으로 붙이는 경우(`Camera3d`의 `DebandDither::Enabled`)에 사용자가 기본값으로 바꿔 두면 불러올 때 요구한 쪽의 값으로 돌아간다.
- 엔티티마다 요구하는 컴포넌트를 요구받는 컴포넌트보다 먼저 쓴다. 불러올 때 파일 순서대로 붙이는데, `Camera`가 `Camera3d`보다 먼저 붙으면 Bevy가 렌더 그래프가 없다고 경고하기 때문이다. Bevy의 직렬화기는 타입 경로순으로 정렬하므로 플러그인이 순서를 지키며 직접 쓴다.
- 파일을 못 읽으면 오류를 로그에 남기고 빈 씬으로 계속한다. 모르는 컴포넌트 타입이 있으면 씬 전체를 불러오지 못한다.
- 에셋 핸들은 불러온 에셋이면 그 경로로 저장된다. 에디터가 씬에 놓은 glTF 모델(`WorldAssetRoot`, 첫 번째 씬)과 씬·프리팹 파일(`DynamicWorldRoot`)은 그 경로만 저장되고, 불러올 때 그 내용이 엔티티 아래에 다시 펼쳐진다. 펼쳐진 내용은 따로 저장하지 않는다. 코드로 만든 에셋의 핸들은 경로가 없어 저장하지 않는다.
- `Children`은 각 자식의 `ChildOf`에서 나오므로 저장하지 않는다. Bevy는 파일을 월드에 쓸 때 관계의 양쪽을 맞추는 훅을 끄므로, 플러그인이 불러온 자식을 부모의 `Children`에 다시 넣는다(씬과 프리팹 모두, 파일에 부모보다 먼저 나와도). 파일에 없는 부모를 가리키는 자식은 경고를 남기고 맨 위로 두고, 저장할 때도 저장되지 않는 부모를 가리키는 `ChildOf`는 쓰지 않는다.
- 놓인 glTF 모델 부품에 에디터가 한 컴포넌트 변경은 루트의 `besfa_editor_plugin::part_changes::PartChanges`(`parts`: `part` 부품 이름 경로, `component` 타입 경로, `value` 컴포넌트 값 전체의 JSON 문자열, 떼어 냈으면 `None`)로 저장한다. 플러그인은 모델이 펼쳐질 때마다 이 변경을 다시 걸고, 같은 이름 경로의 부품이 없으면 건너뛴다.
- Bevy 형식이라 `schema_version`이 없다. 형식의 버전은 Bevy가 관리한다.

## 프리팹 파일

에디터의 Save as prefab은 선택한 엔티티와 그 아래의 저장되는 엔티티를 `assets/prefabs/<이름>.scn.ron`에 씬 파일과 같은 형식으로 쓴다. 뿌리 엔티티는 원점에 놓인 Transform으로, 부모 없이 저장되므로 프리팹을 놓는 자리가 프리팹의 자리가 된다.

놓인 프리팹의 안쪽은 씬 파일에 저장되지 않는다. Apply to prefab은 놓인 프리팹이 펼친 엔티티를 지금 모습 그대로(Transform 포함, 맨 위 엔티티는 부모 없이) 그 프리팹 파일에 다시 쓴다. 에디터가 안쪽에 복제한 씬 엔티티와 지운 엔티티는 쓰지 않고, 안쪽에 놓인 다른 프리팹은 경로만 쓴다. 파일이 자기 자신을 놓게 되는 저장(Save scene, Save as prefab, Apply to prefab 모두)은 불러올 때 끝없이 펼쳐지므로 거절한다. 씬에 놓은 씬 파일 자신에 대한 Apply to prefab도 씬을 그 안쪽으로 덮어쓰므로 거절한다.

## 지형 파일

지형은 씬 파일에 두 컴포넌트로 저장한다. `besfa_editor_plugin::terrain::Terrain`은 크기(`size`, X와 Z로 각각 몇 m인지, 엔티티를 가운데로 둔다)와 겉면 색(`layers`, 네 개까지, 기본값은 풀·흙·돌·모래)이고, `besfa_editor_plugin::terrain::TerrainFile`은 높이와 겉면이 든 파일의 에셋 폴더 기준 경로다. 새 지형은 평평하고 첫 겉면으로 덮인 땅을 메모리에 두었다가 씬을 저장할 때 `assets/terrain/terrain-<번호>.terrain`(디스크에도, 씬의 다른 지형에도 없는 첫 번호)에 쓰므로, 저장하지 않은 지형은 파일을 남기지 않는다.

`.terrain` 파일은 리틀 엔디언 바이너리다.

| 위치 | 내용 |
|---|---|
| 0 | `BTRN` 4바이트 |
| 4 | 형식 버전 `u32`, 지금 1 |
| 8 | 한 변의 표본 수 `u32` (2~1025, 새 지형은 257) |
| 12 | 표본마다 높이 `f32`(m, 엔티티 기준) |
| 이어서 | 표본마다 네 겉면의 비율 `u8` 네 개(합 255) |

표본은 -Z쪽 줄부터 +Z쪽으로, 줄마다 -X에서 +X로 놓인다. 표본 간격은 `size / (표본 수 - 1)`이므로, 크기를 바꾸면 같은 표본이 넓어지거나 좁아진다. 플러그인은 이 파일로 메시를 만들고(꼭짓점 색이 겉면 비율에 따라 섞인 색이다), 메시와 머티리얼은 저장하지 않는다. 붓으로 바꾼 높이와 겉면은 메모리에만 있다가 씬을 저장할 때 그 지형의 파일에 다시 쓴다. 지형을 복제하면 같은 파일을 함께 쓴다.

## 흩뿌린 물건

흩뿌리기로 놓은 물건은 씬 파일에 다른 놓인 모델·프리팹과 같이 저장되고, `besfa_editor_plugin::ground::Scattered` 표식이 붙는다. 흩뿌린 것 지우기는 이 표식이 있는 것만 지우므로 손으로 놓은 물건은 남는다. 흩뿌린 물건은 이름이 `Scatter`이고 `besfa_editor_plugin::ground::ScatterGroup` 표식이 붙은 묶음 아래에 모인다.

## 애니메이션 상태 기계

놓인 glTF 모델의 루트에 붙는 `besfa_editor_plugin::animation::AnimationStateMachine`은 씬 파일에 다른 컴포넌트처럼 저장된다. `states`는 상태 목록이고 첫 상태에서 시작한다. 상태는 이름 `name`, 모델의 이름 붙은 glTF 클립 `clip`, 한 번만 틀고 마지막 자세로 멈추는 `once`(기본 false, 반복), 배속 `speed`(기본 1)를 갖는다. `transitions`는 목록 순서대로 살피는 전이다. 전이는 출발 상태 `from`(`*`이면 도착 상태가 아닌 모든 상태), 도착 상태 `to`, 모두 맞아야 하는 조건 `conditions`, 크로스페이드 초 `blend`(기본 0.2), 출발 상태의 클립을 얼마나 튼 뒤에 넘어갈지 `exit_time`(1이면 한 바퀴, 0이면 언제든, 기본 0)을 갖는다. 조건은 파라미터 이름 `param`, 시험 `test`(`Greater`·`Less`: 실수 파라미터를 `value`와 비교, `True`·`False`: 참거짓 파라미터, `Trigger`: 당긴 트리거, 전이를 타면 놓는다), `value`를 갖는다. 빠진 필드는 모두 기본값이 된다. 상태 기계를 붙이면 `besfa_editor_plugin::animation::AnimationParams`도 함께 붙는다. 게임이 쓰는 `floats`·`bools`·`triggers`와 지금 트는 상태 `state`를 담고, 쓴 적 없는 파라미터는 0과 false로 읽는다. 씬 파일에는 처음 값이 저장된다. 모델의 클립 이름 `AnimationClips`는 플러그인이 계산하므로 저장하지 않는다.

```ron
"besfa_editor_plugin::animation::AnimationStateMachine": (
    states: [
        (name: "idle", clip: "idle"),
        (name: "walk", clip: "walk", speed: 1.2),
    ],
    transitions: [
        (from: "idle", to: "walk", conditions: [(param: "speed", test: Greater, value: 0.1)]),
        (from: "walk", to: "idle", conditions: [(param: "speed", test: Less, value: 0.1)]),
    ],
),
```

## 하늘

씬의 하늘은 엔티티 하나의 `besfa_editor_plugin::sky::Sky` 컴포넌트로 씬 파일에 저장된다([SKY.md](./SKY.md)). `mode`는 `Gradient`(기본)나 `Cubemap`이고, 그라데이션은 머리 위 `zenith`, 지평선 `horizon`, 지평선 아래 `ground` 색과, 지평선 위 높이(0~1)를 몇 제곱해 천정 색으로 섞을지 `blend`(기본 0.5)를 갖는다. 지평선 아래는 높이의 네 배만큼 땅 색으로 섞는다. `cubemap`은 에셋 폴더 기준 이미지 경로로, Bevy가 큐브맵으로 읽는 `.ktx2`·`.dds`나 +X, -X, +Y, -Y, +Z, -Z 여섯 면을 위에서 아래로 쌓은 이미지(높이가 너비의 여섯 배)다. `brightness`(기본 1)는 카메라 노출과 상관없이 고른 색 그대로 보이는 값이 1이고, `rotation`은 위에서 보아 반시계로 돌리는 각도(도)다. 모드를 바꿔도 다른 모드의 값은 남는다. 플러그인은 `Sky`를 씬을 보는 모든 3D 카메라의 Bevy `Skybox`로 그리고(그라데이션은 면마다 64×64 큐브맵으로 만든다), 그 `Skybox`는 저장하지 않는다.

```ron
"besfa_editor_plugin::sky::Sky": (
    mode: Gradient,
    zenith: Srgba((red: 0.2, green: 0.42, blue: 0.78, alpha: 1.0)),
    horizon: Srgba((red: 0.72, green: 0.84, blue: 0.95, alpha: 1.0)),
    ground: Srgba((red: 0.32, green: 0.3, blue: 0.28, alpha: 1.0)),
    blend: 0.5,
    cubemap: "",
    brightness: 1.0,
    rotation: 0.0,
),
```

## 저장 위치

Besfa로 생성한 게임 프로젝트는 씬 외의 에디터 데이터를 프로젝트 루트의 `besfa/` 디렉터리에 저장한다.

```text
my-game/
├─ Cargo.toml
├─ src/
├─ assets/
│  └─ scenes/
│     └─ main.scn.ron
└─ besfa/
   ├─ project.besfa
   ├─ previews/
   ├─ terrain/
   └─ animation/
```

`besfa/`는 디렉터리 이름이고, `project.besfa`는 데이터 파일이다.

## 초기 포맷

- 텍스트 형식: RON
- 파일 확장자: `.besfa`
- 모든 파일은 최상위에 `schema_version`을 가진다.
- 파일은 사람이 직접 편집할 수 있다.
- 파일을 직접 편집한 뒤에는 `besfa validate`로 유효성을 확인한다.

예시:

```ron
(
    schema_version: 1,
    name: "My First Game",
)
```

## 모델 검토 파일

모델 미리보기 탭은 모델마다 고른 것과 바꾼 것을 `besfa/previews/<에셋 경로>.besfa`(예: `besfa/previews/props/chest-wood.glb.besfa`)에 RON으로 저장하고, 그 모델을 다시 열면 그대로 읽는다. 프로젝트에 두므로 함께 커밋하면 다른 사람도 같은 검토 상태로 연다. 탭의 선택이 바뀔 때마다 바뀐 경우에만 쓴다.

```ron
(
    schema_version: 1,
    clip: Some("walk"),   // 클립 이름, 없으면 첫 클립
    rest: false,          // true면 클립 없이 읽었을 때의 자세
    speed: 1.0,
    looping: true,
    light: studio,        // studio, daylight, night
    view: three_quarter,  // front, back, left, right, top, three_quarter
    overlays: (skeleton: false, markers: true, bounds: false),
    hidden: ["Scene/chest-wood/Feet"],  // 루트부터 이어 붙인 부품 이름 경로
    solo: None,
    changes: [            // 부품("" 은 루트)의 컴포넌트에 한 마지막 변경
        (part: "Scene/chest-wood/LidHinge", component: "bevy_transform::components::transform::Transform",
         change: Set({"translation": [0.0, 0.55, 0.32], "rotation": [-0.6, 0.0, 0.0, 0.8], "scale": [1.0, 1.0, 1.0]})),
        (part: "", component: "my_game::Look", change: Insert),
    ],
    originals: [          // 바꾸기 전 값, 없던 컴포넌트는 None. 되돌리기와 자세에 쓴다
        (part: "Scene/chest-wood/LidHinge", component: "bevy_transform::components::transform::Transform",
         value: Some({"translation": [0.0, 0.55, 0.32], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})),
    ],
    poses: [(name: "Open", transforms: {"Scene/chest-wood/LidHinge": {"translation": [0.0, 0.55, 0.32], "rotation": [-0.6, 0.0, 0.0, 0.8], "scale": [1.0, 1.0, 1.0]}})],
)
```

- 컴포넌트 값은 게임이 보고하고 `set`으로 받는 JSON 모양 그대로 RON에 쓴다.
- 빠진 필드는 기본값이다. 이 에디터보다 높은 `schema_version`이나 읽을 수 없는 파일은 패널에 이유를 보이고 덮어쓰지 않는다.
- 같은 이름 경로의 부품이 없어진 변경은 모델을 읽을 때 건너뛰고, 숨김·단독 보기에서는 버린다.

## 버전 관리

포맷을 호환되지 않게 변경할 때는 `schema_version`을 올린다. `besfa_data`는 현재 버전과 이전 지원 버전의 파일을 읽고, `besfa migrate`가 최신 포맷으로 변환한다.

## 구현 상태

씬 파일과 모델 검토 파일(`besfa/previews/`, 에디터의 `review` 모듈이 읽고 쓴다)은 구현되어 있다. `project.besfa`, `besfa_data` crate와 `besfa validate`·`besfa migrate`는 아직 구현되지 않았고, 그 부분은 구현 전에 검토·확정해야 한다.
