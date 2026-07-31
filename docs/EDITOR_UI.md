# Editor UI

`apps/editor_ui`는 Besfa의 Flutter 기반 데스크톱 에디터 UI 프로젝트다.

이 프로젝트는 UI 표시, 사용자 입력, 에디터 상태 표현을 담당한다. Bevy 게임 실행, 프로젝트 파일 생성, 씬 데이터 해석 같은 Rust 도메인 로직은 이 프로젝트에 직접 구현하지 않는다.

## 아키텍처

UI 코드는 [FSD.md](./FSD.md)의 Feature-Sliced Design 규칙을 따른다.

현재는 `app`과 `pages/project_hub`만 구현되어 있다. 나머지 레이어와 slice는 실제 기능을 만들 때 추가한다.

## 현재 상태

- 프로젝트 생성과 열기를 위한 시작 허브 화면
- Windows 최소 창 크기: 900 × 640
- 프로젝트 생성과 열기 기능은 아직 연결되지 않음
