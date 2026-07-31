# Besfa 프로젝트 개요

## 목적

Besfa는 Bevy 게임 엔진을 위한 데스크톱 에디터 프로젝트다.

에디터 UI는 Flutter로 만들고, 게임 프로젝트의 Bevy 런타임과는 별도 프로세스로 연결한다. Flutter는 도킹 UI, 프로젝트 관리, 씬 편집 도구를 담당한다. Bevy는 게임 실행, ECS 상태, 씬 렌더링을 담당한다.

Windows에서는 Bevy가 렌더링한 뷰포트를 D3D 공유 텍스처로 Flutter에 전달하는 방식을 목표로 한다. 연결 경로와 프로세스 수명은 [ARCHITECTURE.md](./ARCHITECTURE.md)를 참고한다.

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

### `apps/editor_ui`

Flutter 기반 Windows 데스크톱 에디터 UI다.

UI의 역할은 [EDITOR_UI.md](./EDITOR_UI.md), FSD-lite 구조와 의존성 규칙은 [FSD.md](./FSD.md)를 참고한다.

- 프로젝트 생성과 열기를 위한 시작 허브 화면
- Windows 최소 창 크기: 900 × 640
- FSD-lite 구조 적용

현재 화면은 기능과 연결되지 않은 UI 골격이다.

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
├─ apps/
│  └─ editor_ui/                   # Flutter 데스크톱 에디터
│     └─ lib/
│        ├─ app/                    # 앱 조립, 테마, 창 초기화
│        ├─ pages/                  # 전체 화면 단위 UI
│        ├─ features/               # 사용자 액션 단위 기능
│        ├─ entities/               # 프로젝트, 씬 등 도메인 모델
│        ├─ widgets/                # 여러 기능을 조합한 큰 UI
│        └─ shared/                 # 공통 UI, FFI, 범용 유틸
│
├─ crates/
│  ├─ besfa_cli/                    # `besfa` 명령행 도구
│  ├─ besfa_project/                # 프로젝트 생성, 검증, 템플릿 처리
│  ├─ besfa_data/                   # .besfa 파일 포맷, 파싱, 검증, 마이그레이션
│  ├─ besfa_protocol/               # 에디터와 게임 런타임 간 통신 규약
│  ├─ besfa_editor_plugin/          # 게임 프로젝트에 추가하는 Bevy Plugin
│  ├─ besfa_viewport_windows/       # D3D 공유 텍스처 기반 Windows 뷰포트
│  └─ besfa_bridge/                 # Flutter FFI와 IPC 연결
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
| `editor_ui` | UI 표시, 사용자 입력, 프로젝트 생성 요청 |
| `besfa_cli` | 터미널과 CI에서 실행하는 명령 인터페이스 |
| `besfa_project` | 프로젝트 생성과 파일 시스템 작업의 실제 규칙 |
| `besfa_data` | 에디터 데이터 파일의 읽기, 쓰기, 검증 |
| `besfa_editor_plugin` | `.besfa` 데이터와 Bevy ECS/Asset의 연결 |
| `besfa_protocol` | 에디터와 실행 중 게임 사이의 메시지 타입 |
| `besfa_viewport_windows` | Bevy 렌더 타깃과 Flutter 뷰포트의 GPU 연결 |

## 설계 문서

- [아키텍처](./ARCHITECTURE.md)
- [Editor UI](./EDITOR_UI.md)
- [Feature-Sliced Design](./FSD.md)
- [CLI 계약](./CLI_CONTRACT.md)
- [에디터 데이터 포맷](./DATA_FORMAT.md)
- [TODO](./TODO.md)
