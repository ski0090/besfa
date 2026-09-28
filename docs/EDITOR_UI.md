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
- 에디터의 Run 버튼은 프로젝트 폴더에서 `cargo run`을 실행하고, 출력은 하단 로그 패널에 표시한다. Stop은 cargo와 게임 프로세스 트리를 함께 종료한다
- 최근 프로젝트 목록은 아직 저장되지 않음
