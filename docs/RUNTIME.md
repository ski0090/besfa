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
| `besfa_components` | `default_plugins()`(`DefaultPlugins` + 아래), 씬 파일 읽기(`ScenePlugin`, `SceneEntity`, `MeshShape`, `MeshColor`, `SCENE_PATH`), 부품 변경 다시 걸기(`PartChanges`), 지형(`Terrain`, `TerrainFile`, `TerrainData`와 로더, 메시 만들기, `TerrainHeight`), 하늘(`Sky`, `Sun`, `Moon`, 렌즈 플레어), 애니메이션 상태 기계와 그 자세 위에 뼈를 돌리고 옮기는 층(`PoseOffsets`), 걷는 클립 없는 모델의 부품을 걸음마다 흔드는 `Gait`, 세계의 자리 위에 붙는 UI(`WorldLabel`), 흩뿌리기 표식(`Scattered`, `ScatterGroup`), 다시 가져오기 이벤트(`Reimported`), `EditorOnly` 표식 |
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
- 공유 target의 Bevy는 cargo가 크레이트마다 toolchain, `Cargo.lock`의 의존성 버전, feature 합집합으로 해시를 매기므로 그 셋이 모두 같은 게임만 재사용한다. 게임의 다른 의존성이 Bevy가 쓰는 크레이트의 feature를 넓히면(duskhold의 rapier·renet은 `downcast-rs`의 `sync`, `profiling`의 `procmacros`, `windows-sys`의 `Win32_System_Time`을) 그 위의 Bevy 전체가 다른 해시로 한 번 더 빌드되고, 그 뒤로는 그 게임의 빌드를 재사용한다. 같은 게임은 늘 같은 feature로 빌드해야 Bevy가 한 벌이다: `besfa build`·`test`·`run`과 에디터는 같은 feature를 쓰고, `besfa_editor` 없이 `cargo run`하면 플러그인이 켜는 Bevy의 `debug` feature가 빠져 Bevy를 또 빌드한다. `RUSTUP_TOOLCHAIN`이나 `CARGO_TARGET_DIR`을 바꿔 띄운 에디터도 마찬가지로 Bevy를 한 벌 더 만든다.
- `cargo update -p besfa_editor_plugin`은 같은 git 저장소의 `besfa_components`도 함께 올리므로, 에디터의 플러그인 갱신 아이콘은 그대로 쓴다.
- `besfa new` 템플릿(`Cargo.toml`, `main.rs`)을 위처럼 바꾼다. 기존 게임(Duskhold)은 고치지 않아도 지금처럼 동작하고, 원할 때 같은 모양으로 옮긴다.

## 테스트와 완료 기준

- 옮긴 테스트가 새 크레이트에서 통과하고, 옛 타입 경로로 쓰인 씬(템플릿 씬, 하늘·지형·상태 기계가 든 씬)을 읽는 테스트를 더한다.
- `cargo test --workspace`, `cargo build --workspace`, `besfa new`로 만든 프로젝트의 빌드, 이 워크트리를 쓰는 Duskhold의 `cargo build`가 통과한다.
- 템플릿 게임을 `besfa_editor` 없이 빌드하면 `cargo tree`에 `besfa_editor_plugin`·`besfa_protocol`이 없다(`windows`·`wgpu`는 Bevy 자체가 쓴다).

## 실행 중 씬 바꾸기

게임은 실행 중에 지금 씬 파일을 떠나 다른 씬 파일로 들어갈 수 있다. duskhold는 새 캐릭터가 울타리 친 튜토리얼 들판(`scenes/tutorial.scn.ron`)에서 시작하고, 끝에서 문을 열면 진짜 들판(`scenes/main.scn.ron`)으로 넘어간다. (duskhold 기획)

| 이름 | 종류 | 하는 일 |
| --- | --- | --- |
| `SwitchScene(path)` | 메시지, 게임이 쓴다 | `assets` 기준 경로의 씬 파일로 넘어가자고 한다 |
| `SceneSpawned(path)` | 메시지, 게임이 읽는다 | 그 씬이 월드에 다 들어왔다 |
| `CurrentScene(path)` | 리소스 | 지금 월드에 있는 씬 파일 |
| `KeepOnSceneSwitch` | 컴포넌트, 게임이 붙인다 | 씬 파일에서 온 엔티티를 다음 씬으로 데려간다 |
| `first_scene()` | 함수 | 게임이 시작하는 씬: `BESFA_SCENE`, 없으면 `scenes/main.scn.ron` |

모두 `besfa_components`에 있고 `default_plugins()`의 `ScenePlugin`이 등록한다.

```rust
use besfa_components::{SceneSpawned, SwitchScene};

fn open_gate(mut switches: MessageWriter<SwitchScene>) {
    switches.write(SwitchScene("scenes/main.scn.ron".into()));
}

fn arrive(mut spawned: MessageReader<SceneSpawned>) {
    for SceneSpawned(path) in spawned.read() {
        // 플레이어를 새 씬의 출발점에 놓는다.
    }
}
```

### 순서

1. 게임이 `SwitchScene`을 쓰면 다음 `PreUpdate`에서 그 파일을 읽기 시작한다. 읽는 동안 지금 씬은 그대로 있고 게임도 그대로 돈다. 다 읽기 전에 또 쓰면 마지막 것만 남는다.
2. 게임의 Startup에서 쓰면 `scenes/main.scn.ron`은 펼쳐지지 않고 그 씬에서 바로 시작한다. 세이브를 불러오는 게임이 이렇게 한다.
3. 파일이 다 읽히면 한 프레임 안에서(`PreUpdate`의 배타 시스템) 새 씬을 펼쳐 `SceneEntity`와 `FileEntity`를 붙이고, 옛 씬의 엔티티를 없애고, `CurrentScene`을 바꾸고, `SceneSpawned`를 보낸다. 옛 땅이 사라지는 프레임에 새 땅이 이미 있으므로, 물리로 서 있는 플레이어가 그사이에 떨어지지 않는다. 자식을 부모의 `Children`에 넣고 메시를 만드는 것은 같은 프레임의 `PostUpdate`다.
4. 파일을 읽거나 펼치지 못하면 오류를 로그에 남기고 옛 씬에 그대로 머문다. `SceneSpawned`는 오지 않는다.
5. 시작할 때 첫 씬이 펼쳐질 때도 `SceneSpawned`가 온다.
6. 지금 있는 씬으로 다시 넘어가면 그 파일을 다시 읽어 처음 모습으로 펼친다.

### 남는 것과 사라지는 것

- 사라지는 것: `SceneEntity`가 붙은 엔티티와 그 아래 전부. 씬 파일에서 온 것, 편집 상태에서 에디터가 만든 것, 숨긴(`Disabled`) 것도 포함한다.
- 남는 것: 게임이 코드로 만든 엔티티(`SceneEntity`가 없다)와 모든 리소스. 플레이어, HUD, 인벤토리처럼 게임이 들고 다니는 것은 코드로 만들면 따로 할 일이 없다.
- 씬 파일에서 온 엔티티를 데려가려면 `KeepOnSceneSwitch`를 붙인다. 붙인 엔티티는 그 아래와 함께 남고, `SceneEntity`와 `FileEntity`를 떼어 게임이 만든 엔티티가 된다. 파일 id는 파일마다 따로 매기므로 새 씬의 엔티티와 겹치고, 세이브(`save_world`)와 에디터의 저장이 그 id로 엔티티를 찾기 때문이다.
- 붙인 엔티티의 부모가 사라지면 맨 위로 나오고 월드의 자리는 그대로다. 게임이 만든 엔티티도 사라지는 씬 엔티티 아래에 있으면 함께 사라지므로, 데려가려면 같은 표식을 붙인다.
- 옛 씬을 위해 게임이 만든 것(튜토리얼의 허수아비 등)은 게임이 `SceneSpawned`를 보고 직접 없앤다.
- `KeepOnSceneSwitch`는 리플렉트하지 않으므로 씬 파일에 들어가지 않는다. 게임이 코드로 붙인다.

### 지금 씬과 세이브

- `CurrentScene`은 씬이 펼쳐질 때 바뀐다. 다음 씬을 읽는 동안에는 아직 옛 씬이고, 게임이 시작해 첫 씬이 펼쳐지기 전에는 없다.
- 월드 세이브(`save_world`, `load_world`)는 씬 엔티티를 그 씬 파일의 id로 찾는다. 다른 씬에서 `load_world`하면 엉뚱한 엔티티에 값이 붙는다. 그래서 게임은 세이브와 함께 `CurrentScene`을 따로 적어 두고(세이브 파일은 리소스를 쓰지 않는다), 불러올 때 그 씬으로 `SwitchScene`한 뒤 `SceneSpawned`에서 `load_world`한다. `SceneSpawned`는 엔티티를 다 붙인 뒤에 오므로 그 자리에서 불러도 된다.
- 씬과 함께 사라지는 `Persist` 씬 엔티티의 값은 남지 않는다. 남겨야 하면 넘어가기 전에 세이브한다.

### 첫 씬

- 게임은 `BESFA_SCENE` 환경 변수(`assets` 기준 경로)가 있으면 그 씬에서, 없으면 `scenes/main.scn.ron`에서 시작한다. 에디터와 `besfa_mcp`가 연 씬을 이 변수로 게임에 알리고, 개발 중에 `BESFA_SCENE=scenes/tutorial.scn.ron cargo run`으로 바로 들어갈 수도 있다.
- 릴리즈 게임에서 어느 씬에서 시작할지는 게임이 정한다. Startup에서 세이브를 읽고 `SwitchScene`을 쓰면 된다(순서 2).

### 편집 상태

- Play 전에는 화면의 씬이 에디터가 연 씬이고 저장하지 않은 변경이 있을 수 있다. 그래서 편집 플러그인은 게임이 쓴 `SwitchScene`을 Play까지 붙잡아 두었다가(`Switching to <path> at Play.` 로그) Play하면 보낸다. 게임의 Startup이 씬을 고르는 흐름도 Play에서 그대로 확인할 수 있다.
- Stop은 게임을 처음부터 다시 시작하므로 연 씬으로 돌아오고, 붙잡아 둔 요청도 버린다.
- `save`는 지금 월드에 있는 씬의 파일에 쓰고, 아직 펼쳐진 씬이 없으면 시작한 씬의 파일에 쓴다(없는 파일이면 만든다). Play 전에는 둘이 같다. 프리팹 저장이 연 씬 파일을 덮어쓰지 않게 막는 것도 연 씬을 따른다.
- 실행 중에 씬이 바뀌면 게임이 그리는 Hierarchy가 새 씬을 보여 준다. 씬을 읽는 동안에는 옛 목록 그대로다.
- 에디터는 Assets 탭에서 `scenes/` 안의 씬 파일을 두 번 누르면 그 씬을 연다. 게임을 그 씬으로 다시 띄우고, 창 제목에 씬 이름이 나온다. 실행 중이거나 저장하지 않은 변경이 있으면 열지 않고 로그에 이유를 남긴다. 프로토콜 18보다 오래된 플러그인은 `BESFA_SCENE`을 모르므로 열지 않는다.
- Play는 연 씬에서 시작한다. net Play의 서버와 클라이언트도 연 씬에서 시작한다.
- `besfa_mcp`의 `start`는 `scene`을 받는다. 없는 파일이면 빈 씬으로 시작하고 처음 `save`가 그 파일을 만든다. 새 씬은 이렇게 만들거나, `scenes/main.scn.ron`을 복사해 만든다.
- 에디터에서 아직 못 하는 것은 [TODO.md](./TODO.md)의 '여러 씬 편집'에 있다.
