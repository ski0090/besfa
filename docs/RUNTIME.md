# 컴포넌트 크레이트 분리 기획

게임 릴리즈에 들어가는 Besfa 컴포넌트(하늘, 지형, 애니메이션 상태 기계 등)를 에디터와 이어 주는 코드와 다른 크레이트로 나눈다.

## 지금

게임은 `besfa_editor_plugin` 하나에 의존하고 `besfa_editor_plugin::default_plugins()`를 `DefaultPlugins` 대신 쓴다. 런타임 플러그인(씬 읽기, 부품 변경, 지형, 하늘, 렌즈 플레어, 애니메이션 상태 기계)은 늘 들어가고, 편집 플러그인은 `BESFA_EDIT_MODE`일 때만 켜진다. 그래서 하늘과 지형은 이미 릴리즈 빌드에서 동작한다.

문제는 한 크레이트에 둘이 섞여 있다는 것이다.

- 릴리즈 빌드도 에디터 전용 의존성을 끌어온다: `besfa_protocol`, D3D12 공유 텍스처용 `windows`·`wgpu`, 컴포넌트·시스템 이름을 남기는 Bevy `debug` feature.
- 지형(`terrain.rs`), 씬(`scene.rs`), 부품 변경(`part_changes.rs`), 흩뿌리기(`ground.rs`)는 한 파일에 게임이 쓰는 부분과 에디터 붓·저장·보고가 함께 있다.

## 나누는 법

새 크레이트 `besfa_components`를 만든다. `besfa_editor_plugin`은 이 크레이트에 의존하고 그 타입을 다시 내보내므로, 게임 코드의 `besfa_editor_plugin::{Sky, Terrain, TerrainHeight, ...}`는 그대로 컴파일된다.

| 크레이트 | 담는 것 |
| --- | --- |
| `besfa_components` | `default_plugins()`(`DefaultPlugins` + 아래), 씬 파일 읽기(`ScenePlugin`, `SceneEntity`, `MeshShape`, `MeshColor`, `SCENE_PATH`), 부품 변경 다시 걸기(`PartChanges`), 지형(`Terrain`, `TerrainFile`, `TerrainData`와 로더, 메시 만들기, `TerrainHeight`), 하늘(`Sky`, `Sun`, `Moon`, 렌즈 플레어), 애니메이션 상태 기계와 그 자세 위에 뼈를 돌리고 옮기는 층(`PoseOffsets`), 세계의 자리 위에 붙는 UI(`WorldLabel`), 흩뿌리기 표식(`Scattered`, `ScatterGroup`), 다시 가져오기 이벤트(`Reimported`), `EditorOnly` 표식 |
| `besfa_editor_plugin` | `default_plugins()`(`besfa_components::default_plugins()` + 편집 플러그인), 편집 상태, 프로토콜, 인스펙터 보고, 씬·프리팹 저장, 지형 붓과 `.terrain` 쓰기, 흩뿌리기 붓, 땅에 내려놓기, 미리보기·썸네일·스크린샷, 뷰포트 텍스처 |

- `besfa_components`는 `bevy`(기본 feature)와 `serde_json`(부품 변경 값이 JSON이다)만 쓴다. `besfa_protocol`, `windows`, `wgpu`, `debug` feature는 쓰지 않는다.
- 섞인 파일은 런타임 부분만 옮기고 편집 부분은 `besfa_editor_plugin`에 남긴다. 런타임 쪽은 편집 쪽이 쓰는 함수(`scene::asset_file`, 지형 표본 읽기 등)를 `pub`으로 열거나 `#[doc(hidden)]`으로 내보낸다.

## 씬 파일 호환

씬 파일은 컴포넌트를 타입 경로로 적는다(`besfa_editor_plugin::sky::Sky`). 크레이트를 옮기면 경로가 바뀌어 이미 저장된 씬(Duskhold의 `main.scn.ron` 포함)을 읽지 못한다. 그래서 옮기는 타입에 `#[type_path = "besfa_editor_plugin::sky"]`처럼 옛 모듈 경로를 붙여 경로를 그대로 둔다. 파일도, 에디터가 컴포넌트를 부르는 이름(`besfa_editor::sky::SKY` 등)도, `besfa_mcp` 안내문도 바꾸지 않는다. 경로를 바꾸는 마이그레이션은 하지 않는다.

## 릴리즈에서 에디터 플러그인 빼기(선택)

크레이트를 나누기만 하면 게임은 여전히 `besfa_editor_plugin`에 의존하므로 릴리즈 빌드도 에디터 의존성을 끌어온다. 이것까지 빼려면 게임이 런타임 크레이트에만 늘 의존하고, 에디터 플러그인은 cargo feature로 켠다.

```toml
[dependencies]
besfa_components = { git = "https://github.com/ski0090/besfa" }
besfa_editor_plugin = { git = "https://github.com/ski0090/besfa", optional = true }

[features]
besfa_editor = ["dep:besfa_editor_plugin"]
```

```rust
#[cfg(feature = "besfa_editor")]
let plugins = besfa_editor_plugin::default_plugins();
#[cfg(not(feature = "besfa_editor"))]
let plugins = besfa_components::default_plugins();
```

- 에디터, `besfa_mcp`, `besfa new`의 미리 빌드는 게임을 `--features bevy/dynamic_linking,besfa_editor`로 실행한다. 그 feature가 없는 옛 프로젝트는 cargo가 실패하므로 `Cargo.toml`의 `[features]`에 `besfa_editor`가 있을 때만 붙인다(`besfa_protocol::run_features`).
- `cargo update -p besfa_editor_plugin`은 같은 git 저장소의 `besfa_components`도 함께 올리므로, 에디터의 플러그인 갱신 아이콘은 그대로 쓴다.
- `besfa new` 템플릿(`Cargo.toml`, `main.rs`)을 위처럼 바꾼다. 기존 게임(Duskhold)은 고치지 않아도 지금처럼 동작하고, 원할 때 같은 모양으로 옮긴다.

## 테스트와 완료 기준

- 옮긴 테스트가 새 크레이트에서 통과하고, 옛 타입 경로로 쓰인 씬(템플릿 씬, 하늘·지형·상태 기계가 든 씬)을 읽는 테스트를 더한다.
- `cargo test --workspace`, `cargo build --workspace`, `besfa new`로 만든 프로젝트의 빌드, 이 워크트리를 쓰는 Duskhold의 `cargo build`가 통과한다.
- 템플릿 게임을 `besfa_editor` 없이 빌드하면 `cargo tree`에 `besfa_editor_plugin`·`besfa_protocol`이 없다(`windows`·`wgpu`는 Bevy 자체가 쓴다).
