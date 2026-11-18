# TODO

## 우선순위

- `P0` — 다음 구현을 시작하기 전에 확정해야 하는 계약

## P0 — 구현 전 계약 확정

- [ ] **P0** [CLI_CONTRACT.md](./CLI_CONTRACT.md)의 JSON 결과, 종료 코드, 대상 경로 정책을 검토하고 확정한다.
- [ ] **P0** [DATA_FORMAT.md](./DATA_FORMAT.md)의 `.besfa` 위치, 포맷, 스키마 버전을 검토하고 확정한다.

## 여러 씬 편집

게임이 실행 중에 씬을 바꾸는 것과, 에디터가 Assets 탭에서 두 번 눌러 다른 씬을 열고 그 씬에서 Play하는 것은 된다([RUNTIME.md](./RUNTIME.md)의 '실행 중 씬 바꾸기'). 에디터에 아직 없는 것:

- [ ] File 메뉴의 New scene, Open scene, Save scene as. 지금은 씬 파일을 복사하거나 `besfa_mcp`의 `start`에 없는 `scene`을 주고 `save`해서 만든다.
- [ ] 저장하지 않은 변경이 있을 때 Save, Discard, Cancel을 묻고 씬 열기. 지금은 로그만 남기고 열지 않는다. 묻게 하면 `AfterSave`에 열기가 더해지므로 `verify-scene-saving`의 모델도 함께 고친다.
- [ ] 프로젝트를 다시 열면 마지막으로 연 씬 열기. 지금은 늘 `scenes/main.scn.ron`이다.
- [ ] Hierarchy 머리의 씬 이름, 그리고 실행 중 게임이 씬을 바꾸면 창 제목과 함께 바뀌기. 게임이 `CurrentScene`을 에디터에 알리는 보고가 필요하다. 지금 창 제목은 연 씬 이름 그대로다.
- [ ] Inspector의 Apply to prefab 단추는 아직 `scenes/main.scn.ron`을 놓은 것에서만 숨는다. 다른 씬을 열었을 때 그 씬을 놓은 것에서 누르면 게임이 거절한다.
- [ ] 프리팹 파일을 씬처럼 열어 고치기.
