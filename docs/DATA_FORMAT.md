# 에디터 데이터 포맷

이 문서는 `besfa_data` crate를 구현하기 전에 확정할 초기 `.besfa` 데이터 포맷의 초안이다. 씬은 예외로, 아래 '씬 파일'대로 Bevy의 월드 형식을 그대로 쓴다.

## 씬 파일

게임의 씬은 프로젝트의 에셋 폴더 안 `assets/scenes/main.scn.ron`에 Bevy의 월드 형식(`bevy_world_serialization`의 RON)으로 저장한다. 게임의 `besfa_editor_plugin`이 시작할 때 이 파일을 읽어 엔티티를 만들고(`SceneEntity` 표식을 붙인다), 에디터의 File > Save scene은 그 엔티티들을 같은 파일에 다시 쓴다. `besfa new`가 큐브·조명·카메라가 든 파일을 만들고, 사람이 직접 고쳐도 된다.

- 엔티티마다 `Reflect`로 등록된 컴포넌트만 들어간다. 게임 자신의 컴포넌트는 `my_game::Spin`처럼 크레이트 이름이 앞에 붙는다.
- 쓰지 않은 필드는 기본값을 쓴다 (`#[reflect(Default)]` 타입).
- 메시와 머티리얼은 핸들이라 파일에 넣을 수 없으므로, 플러그인의 `MeshShape`(상자·구·평면)와 `MeshColor`를 넣고 플러그인이 메시와 머티리얼을 만든다. 둘이 바뀌면 다시 만들고, 떼어 내면 메시나 머티리얼도 뗀다. 둘 다 기본값이 있어 에디터에서 붙일 수 있다.
- 저장할 때는 핸들, `GlobalTransform`·가시성·`Aabb`·`Frustum`처럼 다른 컴포넌트에서 계산되는 것, 직렬화되지 않는 컴포넌트를 뺀다 (직렬화되지 않는 것은 경고를 남긴다). 카메라의 투영처럼 Bevy가 기본값으로 채운 컴포넌트는 그대로 쓴다.
- 파일을 못 읽으면 오류를 로그에 남기고 빈 씬으로 계속한다. 모르는 컴포넌트 타입이 있으면 씬 전체를 불러오지 못한다.
- 에셋 핸들은 불러온 에셋이면 그 경로로 저장된다. 에디터가 씬에 놓은 glTF 모델(`WorldAssetRoot`, 첫 번째 씬)과 씬·프리팹 파일(`DynamicWorldRoot`)은 그 경로만 저장되고, 불러올 때 그 내용이 엔티티 아래에 다시 펼쳐진다. 펼쳐진 내용은 따로 저장하지 않는다. 코드로 만든 에셋의 핸들은 경로가 없어 저장하지 않는다.
- `Children`은 각 자식의 `ChildOf`에서 나오므로 저장하지 않는다.
- Bevy 형식이라 `schema_version`이 없다. 형식의 버전은 Bevy가 관리한다.

## 프리팹 파일

에디터의 Save as prefab은 선택한 엔티티와 그 아래의 저장되는 엔티티를 `assets/prefabs/<이름>.scn.ron`에 씬 파일과 같은 형식으로 쓴다. 뿌리 엔티티는 원점에 놓인 Transform으로, 부모 없이 저장되므로 프리팹을 놓는 자리가 프리팹의 자리가 된다.

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

## 버전 관리

포맷을 호환되지 않게 변경할 때는 `schema_version`을 올린다. `besfa_data`는 현재 버전과 이전 지원 버전의 파일을 읽고, `besfa migrate`가 최신 포맷으로 변환한다.

## 구현 상태

씬 파일은 구현되어 있다. `besfa_data` crate와 실제 `.besfa` 파서는 아직 구현되지 않았고, 그 부분은 구현 전에 검토·확정해야 한다.
