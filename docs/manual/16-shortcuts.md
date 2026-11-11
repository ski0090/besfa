[← 사용 설명서 목차](README.md)

# 단축키

편집 상태에서 쓰는 단축키입니다. **실행 중(Play)에는 키가 게임으로 가므로 에디터 단축키가 꺼집니다.** 글자 입력 칸에 커서가 있을 때는 Ctrl+S와 Ctrl+9 말고는 글자 편집에 쓰입니다. Inspector의 칸도 같습니다. 칸 바깥을 한 번 누르면 단축키가 돌아옵니다.

## 파일과 도움말

| 키 | 하는 일 | 설명 |
| --- | --- | --- |
| **Ctrl+S** | 씬 저장 | [실행과 저장](05-play-and-save.md#씬-저장) |
| **Ctrl+9** | Bevy 커뮤니티 에셋 목록을 브라우저로 열기 | [화면 둘러보기](02-screen-layout.md#1-메뉴) |

## 편집

| 키 | 하는 일 | 설명 |
| --- | --- | --- |
| **Ctrl+Z** | 되돌리기 | [되돌리기](06-undo.md) |
| **Ctrl+Y**, **Ctrl+Shift+Z** | 다시 하기 | [되돌리기](06-undo.md) |
| **Ctrl+D** | 고른 엔티티 복제 | [엔티티와 Inspector](03-hierarchy-inspector.md#복제와-삭제) |
| **Delete** | 고른 엔티티 삭제(자식 포함) | [엔티티와 Inspector](03-hierarchy-inspector.md#복제와-삭제) |
| **End** | 고른 엔티티를 아래 땅에 내려놓기 | [씬 뷰 조작](04-scene-view.md#땅에-내려놓기) |
| **Esc** | 지형 붓과 흩뿌리기 붓 끄기 | [지형](09-terrain.md#붓-끄기), [흩뿌리기](10-scatter.md) |

## 씬 뷰

| 키·마우스 | 하는 일 | 설명 |
| --- | --- | --- |
| **W** / **E** / **R** | 이동 / 회전 / 크기 핸들 | [씬 뷰 조작](04-scene-view.md#이동회전크기-핸들) |
| **F** | 고른 엔티티로 다가가기 | [씬 뷰 조작](04-scene-view.md#카메라-움직이기) |
| 왼쪽 클릭 | 고르기(빈 곳은 고르기 풀기) | |
| 오른쪽 끌기 | 돌기 | |
| 가운데 끌기 | 옮기기 | |
| 휠 | 다가가기, 멀어지기 | |
| 오른쪽 버튼 + **W A S D** | 앞·왼쪽·뒤·오른쪽으로 날기 | |
| 오른쪽 버튼 + **Q** / **E** | 아래 / 위로 날기 | |
| 날면서 **Shift** | 세 배 빠르게 | |
| 핸들을 끌면서 **Ctrl** | 맞춤(Snap) | [맞춤](04-scene-view.md#맞춤snap) |
| 붓으로 칠하면서 **Shift** | 지형: Raise와 Lower 바꾸기 / 흩뿌리기: 지우기 | [지형](09-terrain.md), [흩뿌리기](10-scatter.md) |

## 실행

| 키·마우스 | 하는 일 | 설명 |
| --- | --- | --- |
| **Ctrl** + **⏹ Stop** 클릭 | 게임을 끄고 다시 실행하기 | [실행과 저장](05-play-and-save.md#play와-stop) |

## 다른 창

| 창 | 키·마우스 | 하는 일 |
| --- | --- | --- |
| Animation State Machine | **Ctrl+S**, **Ctrl+Z**, **Ctrl+Y**, **Ctrl+Shift+Z** | 에디터 본 창과 같이 저장, 되돌리기, 다시 하기. [애니메이션 상태 기계](11-animation.md) |
| Agent 창 **Chat** | **Ctrl+Enter** | 입력란의 글 보내기. [Agent 창](15-agent-window.md#chat-탭) |
| Agent 창 **Organization** | **Shift**+클릭 | 에이전트 여러 개 고르기 |
| | 핀 오른쪽 클릭 | 연결 끊기 |
| | 휠 | 확대·축소. [Organization 탭](15-agent-window.md#organization-탭) |
| Assets 탭 | 모델 두 번 클릭 | 모델 미리보기 열기. [모델 미리보기](08-model-preview.md) |

## 모델 미리보기

| 키·마우스 | 하는 일 |
| --- | --- |
| 왼쪽 또는 오른쪽 끌기 | 돌기 |
| 가운데 끌기 | 옮기기 |
| 휠 | 다가가기, 멀어지기 |
| **F** | 모델에 다시 맞추기 |

미리보기 중에는 Ctrl+S, Ctrl+9, F만 듣고 Delete, Ctrl+D, 되돌리기, W·E·R, End, Esc는 쉽니다. 되돌리기는 Animation State Machine 창에서는 미리보기 중에도 됩니다. [모델 미리보기](08-model-preview.md)

---

다음: [문제 해결](17-troubleshooting.md)
