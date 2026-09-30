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
- 에디터의 Run 버튼은 프로젝트 폴더에서 `cargo run --features bevy/dynamic_linking`을 실행하고 (dynamic linking은 에디터 실행에만 쓰고, 일반 `cargo build`는 단독 실행 파일을 만든다), 출력은 하단 로그 패널에 표시한다. Stop은 cargo와 게임 프로세스 트리를 함께 종료한다
- 뷰포트: Run 시 `windows/runner/viewport_texture.cpp`가 1280×720 D3D11 공유 텍스처를 만들고, 이름을 `BESFA_VIEWPORT` 환경 변수로 게임에 넘긴다. 게임의 `besfa_editor_plugin`이 이 텍스처를 D3D12로 열어 렌더링하고, 에디터는 매 프레임 Flutter용 텍스처로 복사해 `Texture` 위젯에 표시한다. 텍스처를 만들 수 없으면 게임은 자체 창으로 실행된다
- 최근 프로젝트 목록: 연 프로젝트를 `%APPDATA%\Besfa\recent_projects.json`에 최신순으로 최대 10개 저장한다. 목록에서 연 프로젝트에 `Cargo.toml`이 없으면 목록에서 제거한다
