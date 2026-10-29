# Besfa 프로젝트 개요

## 목적

Besfa는 Bevy 게임 엔진을 위한 데스크톱 에디터 프로젝트다.

에디터 UI는 Rust와 egui로 만들고, 게임 프로젝트의 Bevy 런타임과는 별도 프로세스로 연결한다. 에디터는 패널 UI, 프로젝트 관리, 씬 편집 도구를 담당한다. Bevy는 게임 실행, ECS 상태, 씬 렌더링을 담당한다.

Windows에서는 Bevy가 에디터의 D3D12 공유 텍스처에 뷰포트를 렌더링한다. 연결 경로와 프로세스 수명은 [ARCHITECTURE.md](./ARCHITECTURE.md)를 참고한다.

## 프로젝트 경계

Besfa 에디터와 Besfa로 생성한 게임은 서로 독립된 프로젝트다.

- 이 저장소는 에디터, CLI, 공통 SDK를 관리한다.
- 사용자가 만드는 게임은 별도 폴더와 별도 Git 저장소를 가진다.
- 게임 프로젝트는 필요한 Besfa Rust crate만 의존성으로 사용한다.

```text
besfa/                         # 이 저장소
└─ templates/                  # 향후 게임 생성용 템플릿

my-game/                       # 에디터가 생성하는 별도 사용자 프로젝트
├─ Cargo.toml
├─ src/
├─ assets/
└─ besfa/                      # .besfa 프로젝트 데이터
```

## 현재 구성

### `crates/besfa_editor`

egui(eframe, wgpu DX12) 기반 Windows 데스크톱 에디터다. 프로젝트 허브와, 게임을 뷰포트에서 실행하며 씬을 고치는 편집 화면이 있다. 역할과 동작은 [EDITOR_UI.md](./EDITOR_UI.md)를 참고한다.

### `crates/besfa_protocol`

에디터와 게임의 `besfa_editor_plugin`이 함께 쓰는 메시지 타입이다. 에디터가 게임의 stdin에 쓰는 `Request`(`Command`와 답을 받을 `seq`)와 게임이 stdout에 쓰는 `Report`, 그리고 프로토콜 버전이 있다. 자세한 규칙은 [ARCHITECTURE.md](./ARCHITECTURE.md)를 참고한다.

### `crates/besfa_components`

게임이 릴리즈에 담는 컴포넌트와 그 플러그인이다([RUNTIME.md](./RUNTIME.md)). 게임은 `besfa_components::default_plugins()`를 `DefaultPlugins` 대신 쓰고, 이것이 씬 파일을 읽고 부품 변경, 지형, 하늘과 렌즈 플레어, 애니메이션 상태 기계를 돌린다. 의존성은 Bevy와 serde뿐이라 에디터 쪽 코드(프로토콜, 뷰포트 공유 텍스처, Bevy `debug` feature)를 끌어오지 않는다. 컴포넌트의 타입 경로는 옮기기 전과 같은 `besfa_editor_plugin::sky::Sky` 등이라 그 전에 저장한 씬도 그대로 읽는다.

### `crates/besfa_editor_plugin`

에디터가 게임을 띄울 때 쓰는 Bevy 플러그인이다. `besfa_components`에 의존해 `default_plugins()`가 그 위에 편집 플러그인을 더하고, 에디터가 띄우면 편집 상태로 시작해 에디터의 명령을 받으며 씬을 저장한다. `besfa new`로 만든 게임은 이 크레이트를 `besfa_editor` feature로만 켜므로(에디터, `besfa_mcp`, 미리 빌드가 그 feature로 실행한다) 릴리즈 빌드에는 들어가지 않는다. 그 전에 만든 게임은 이 크레이트에 바로 의존하고, 이 크레이트가 `besfa_components`의 타입을 다시 내보내므로 그대로 빌드된다. 게임 코드가 쓰는 것으로 지형이 있다: 지형은 `Terrain`(크기와 겉면 색)과 `TerrainFile`(높이 파일 경로)로 씬에 저장되고, 게임 코드는 시스템 인자 `TerrainHeight`의 `at(x, z)`로 그 자리의 땅 높이를 묻는다(캐릭터와 좀비를 땅 위에 세울 때). 흩뿌리기로 놓은 물건에는 `Scattered` 표식이 있어 게임이 골라낼 수 있다. 씬의 하늘은 `Sky` 컴포넌트(그라데이션 색이나 큐브맵 이미지)이고, 게임 코드가 `Query<&mut Sky>`로 값을 바꾸면 플러그인이 모든 카메라의 하늘을 따라 바꾼다([SKY.md](./SKY.md)). 지형이나 그 아래 엔티티의 `Foliage`는 지형의 겉면 레이어 하나가 덮은 만큼 풀 같은 식물을 카메라 가까이에만 기른다([FOLIAGE.md](./FOLIAGE.md)).

### `crates/besfa_net`

게임이 데디케이트 서버를 여는 데 쓰는 크레이트다. 서버는 창과 GPU 없이 도는 같은 게임이고(`headless(plugins, hz)`: `besfa_components::windowless`에 GPU를 고르지 않는 `RenderPlugin`을 더한다), `Server::listen(port, protocol, max_clients)`로 UDP 포트를 열며, 각 게임은 `Client::connect(address, protocol)`로 들어간다. 연결은 renet 2.0과 renet_netcode 2.0이고, `Server`와 `Client`는 renet의 서버와 클라이언트로 deref되므로 게임은 renet의 메시지를 그대로 읽고 보낸다. 게임은 매 프레임 `update(delta)`로 받고, 메시지를 처리한 뒤 `send_packets()`로 보내며, 끝날 때 `close()`로 상대에게 바로 알린다. 무엇을 주고받는지(들어오기 인사, 버전 확인, 엔티티 동기화)는 게임이 정하고, `encode`/`decode`(JSON)로 메시지를 바이트로 바꾼다. 엔티티 하나를 텍스트로 쓰고 받는 쪽이 허락한 컴포넌트만 다시 붙이는 일은 `besfa_components::write_entity`/`read_entity`가 한다. renet과 renet_netcode는 이 크레이트가 다시 내보내므로(`besfa_net::renet`) 서버와 게임이 같은 판을 쓴다. Bevy 플러그인(bevy_renet)이 아직 Bevy 0.19에 머물러 있어 그 시스템들을 게임이 부르는 메서드로 두었고, bevy_renet이 이 Bevy를 지원하면 그쪽으로 바꾼다. 혼자 하는 게임만 내는 프로젝트는 이 크레이트에 의존하지 않으면 된다.

### `crates/besfa_mcp`

에이전트(Claude Code 등)가 에디터 UI 없이 게임의 씬을 읽고 고치게 하는 stdio MCP 서버다. 에디터의 Run처럼 게임을 edit mode로 띄우고(뷰포트도 창도 없이, `BESFA_HEADLESS`로 사용자 포커스를 빼앗지 않는다. 모델 썸네일은 오프스크린으로 그대로 그려진다) `besfa_protocol` 명령을 그대로 전달한다. 도구는 `start`, `send`(명령 하나나 목록을 차례로 보내고, `screenshot`·`thumbnail`·`reimport`는 답이 올 때까지 기다린다. 게임의 프로토콜이 모르는 명령은 보내기 전에 거절한다), `entities`(이름·에셋·부모로 거르고, 놓인 모델이 펼친 부품은 빼거나 함께 본다, `id`면 그 엔티티의 컴포넌트), `stop`이고, 게임 프로젝트에서는 `besfa new`가 넣는 `.mcp.json`이 `besfa mcp`로 띄우고(`start`에 `project`가 없으면 그 폴더), 이 저장소에서는 루트의 `.mcp.json`이 `cargo run -q -p besfa_mcp`로 띄운다. 서버는 라이브러리(`besfa_mcp::serve`)라서 CLI와 이 크레이트의 실행 파일이 함께 쓴다. 에디터가 열어 둔 프로젝트에 같이 쓰면 두 쪽이 씬 파일을 덮어쓴다.

### `crates/besfa_cli`

Besfa 프로젝트 자동화를 위한 Rust 바이너리 crate다.

현재 정의된 명령은 다음과 같다.

```text
besfa new <DIRECTORY> [--template <TEMPLATE>]
besfa validate [DIRECTORY]
besfa --help
besfa --version
```

`new`와 `validate`는 인자 파싱과 도움말만 구현되어 있으며, 실제 파일 생성과 검증은 아직 구현하지 않았다.

## 예정 폴더 구조

```text
besfa/
├─ crates/
│  ├─ besfa_editor/                 # egui 데스크톱 에디터
│  ├─ besfa_cli/                    # `besfa` 명령행 도구
│  ├─ besfa_project/                # 프로젝트 생성, 검증, 템플릿 처리
│  ├─ besfa_data/                   # .besfa 파일 포맷, 파싱, 검증, 마이그레이션
│  ├─ besfa_protocol/               # 에디터와 게임 런타임 간 통신 규약
│  ├─ besfa_components/             # 게임이 릴리즈에 담는 컴포넌트와 플러그인
│  ├─ besfa_editor_plugin/          # 에디터가 게임을 띄울 때 더하는 Bevy Plugin
│  ├─ besfa_net/                    # 데디케이트 서버와 그 연결
│  └─ besfa_viewport_windows/       # D3D 공유 텍스처 기반 Windows 뷰포트
│
├─ templates/
│  └─ basic_3d/                     # 첫 게임 프로젝트 템플릿
│
├─ docs/                            # 설계와 개발 문서
├─ Cargo.toml                       # Rust workspace
└─ Cargo.lock
```

빈 디렉터리는 미리 만들지 않는다. 각 기능을 실제로 시작할 때 생성한다.

## 책임 분리

| 구성 요소 | 책임 |
| --- | --- |
| `besfa_editor` | UI 표시, 사용자 입력, 프로젝트 생성 요청, 게임 프로세스 관리 |
| `besfa_cli` | 터미널과 CI에서 실행하는 명령 인터페이스 |
| `besfa_project` | 프로젝트 생성과 파일 시스템 작업의 실제 규칙 |
| `besfa_data` | 에디터 데이터 파일의 읽기, 쓰기, 검증 |
| `besfa_components` | 게임이 릴리즈에 담는 컴포넌트(씬 파일, 지형, 하늘, 애니메이션 상태 기계) |
| `besfa_editor_plugin` | `.besfa` 데이터와 Bevy ECS/Asset의 연결, 편집 상태 |
| `besfa_net` | 데디케이트 서버: 창과 GPU 없이 돌리기, 서버와 게임 사이의 UDP 연결 |
| `besfa_protocol` | 에디터와 실행 중 게임 사이의 메시지 타입 |
| `besfa_viewport_windows` | Bevy 렌더 타깃과 에디터 뷰포트의 GPU 연결 |

## 설계 문서

- [아키텍처](./ARCHITECTURE.md)
- [Editor UI](./EDITOR_UI.md)
- [CLI 계약](./CLI_CONTRACT.md)
- [에디터 데이터 포맷](./DATA_FORMAT.md)
- [하늘 편집](./SKY.md)
- [폴리지](./FOLIAGE.md)
- [컴포넌트 크레이트 분리](./RUNTIME.md)
- [TODO](./TODO.md)
