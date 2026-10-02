# Editor UI

`apps/editor_ui`는 Besfa의 Flutter 기반 데스크톱 에디터 UI 프로젝트다.

이 프로젝트는 UI 표시, 사용자 입력, 에디터 상태 표현을 담당한다. Bevy 게임 실행, 프로젝트 파일 생성, 씬 데이터 해석 같은 Rust 도메인 로직은 이 프로젝트에 직접 구현하지 않는다.

## 아키텍처

UI 코드는 [FSD.md](./FSD.md)의 Feature-Sliced Design 규칙을 따른다.

나머지 레이어와 slice는 실제 기능을 만들 때 추가한다. 페이지 사이의 이동은 `app`의 `onGenerateRoute`에서 조립한다.

## 현재 상태

- 프로젝트 생성과 열기를 위한 시작 허브 화면
- Windows 최소 창 크기: 900 × 640
- 새 프로젝트 생성 대화상자에서 대상 경로를 입력하면 PATH의 `besfa` CLI를 `new <DIRECTORY> --output json`으로 실행
- CLI의 JSON 성공·실패 결과를 UI에 표시
- 생성에 성공하면 해당 프로젝트로 에디터 화면(`pages/project_editor`)을 연다
- 프로젝트 열기: 폴더를 선택하면 `Cargo.toml`이 있는지 확인한 뒤 에디터 화면을 연다
- 편집 상태와 실행 상태: 에디터는 프로젝트를 열면 프로젝트 폴더에서 `cargo run --features bevy/dynamic_linking`을 `BESFA_EDIT_MODE=1`로 실행한다 (dynamic linking은 에디터 실행에만 쓰고, 일반 `cargo build`는 단독 실행 파일을 만든다). 게임의 `besfa_editor_plugin`은 이 변수를 보면 `Time<Virtual>`을 멈춘 채 시작하므로, Startup이 만든 씬이 뷰포트에 멈춘 채로 보인다(편집 상태). Play는 게임 stdin에 `play` 한 줄을 보내 시간을 다시 흐르게 하고(실행 상태, 로그도 비운다), Stop은 cargo와 게임 프로세스 트리를 종료한 뒤 편집 상태로 다시 실행한다. 다시 실행하므로 씬이 초기 상태로 돌아가고 코드 변경도 반영된다. cargo가 아직 빌드 중일 때 누른 Play는 파이프에 남았다가 게임이 시작되면 읽힌다. 편집 세션이 빌드 실패나 크래시로 끝났으면 Play가 게임을 실행 상태로 바로 시작한다. 출력은 하단 로그 패널에 표시한다. 시간에 의존하지 않거나 `Time<Real>`을 읽는 시스템은 편집 상태에서도 돈다. 에디터 프로세스는 시작할 때 자신을 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` Job Object에 넣으므로(`windows/runner/main.cpp`), 에디터가 어떻게 종료되든(창 닫기, 크래시, 강제 종료) 빌드 중인 cargo·rustc와 창 없는 게임도 함께 종료된다
- Bevy 미리 빌드: 에디터가 시작하면 백그라운드에서 `besfa prebuild`를 실행해 공유 target 디렉터리에 Bevy를 에디터의 게임 실행과 같은 feature로 빌드한다. 그래서 `besfa new`로 만든 게임의 실행은 미리 빌드한 Bevy를 재사용하고 게임 코드만 컴파일한다. 미리 빌드가 진행 중인 동안 편집 세션은 시작하지 않고 Play 버튼도 비활성화된다(어차피 cargo가 빌드 디렉터리 잠금을 기다려야 한다). 미리 빌드가 끝나면 편집 세션을 시작한다. 진행 상황은 허브 하단과 에디터 툴바에 한 줄로 표시한다: 실행 중에는 cargo의 마지막 출력 줄과 진행률(`CARGO_TERM_PROGRESS_WHEN=always`로 파이프에서도 켠 cargo 진행 막대의 `완료/전체` 빌드 단위, 이미 빌드된 단위도 완료로 센다), 끝나면 `Bevy is ready` 또는 실패한 종료 코드
- 뷰포트: 게임을 실행할 때 `windows/runner/viewport_texture.cpp`가 1280×720 D3D11 공유 텍스처를 만들고, 이름을 `BESFA_VIEWPORT` 환경 변수로 게임에 넘긴다. 게임의 `besfa_editor_plugin`이 이 텍스처를 D3D12로 열어(크기와 포맷은 열린 리소스의 `GetDesc()`로 확인한다) 렌더링하고, 에디터는 매 프레임 Flutter용 텍스처로 복사해 `Texture` 위젯에 표시한다. 텍스처를 만들 수 없으면 게임은 자체 창으로 실행된다. 게임이 `BESFA_VIEWPORT`를 받았는데 텍스처를 열지 못하면, 오류를 로그에 남기고 종료 코드 1로 끝난다(템플릿의 `main`은 `AppExit`을 반환한다)
- 최근 프로젝트 목록: 연 프로젝트를 `%APPDATA%\Besfa\recent_projects.json`에 최신순으로 최대 10개 저장한다. 목록에서 연 프로젝트에 `Cargo.toml`이 없으면 목록에서 제거한다. 항목의 폴더 아이콘은 탐색기에서 프로젝트 폴더를 열고, 삭제 아이콘은 확인 후 프로젝트 폴더를 휴지통으로 옮기고 목록에서 제거한다 (`Cargo.toml`이 없는 폴더는 지우지 않고 목록에서만 제거)
