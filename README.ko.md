# Besfa

[English](README.md) | **한국어**

> 📖 **[사용 설명서](docs/manual/README.md)**: 설치, 프로젝트 만들기, 에디터의 모든 부분을 스크린샷과 함께 차근차근 설명합니다.

Besfa는 Windows에서 쓰는 [Bevy](https://bevy.org/) 게임 엔진용 데스크톱 에디터입니다. 내 게임이 에디터 안에서 실행됩니다. 뷰포트로 게임을 보면서, 멈춘 상태로 물건을 놓고 값을 고치고, 실행해 보고, 씬을 파일에 저장합니다. 에디터 UI는 egui, 프로젝트 관리는 `besfa` CLI가 맡고, 둘 다 Rust로 작성합니다.

![Besfa 에디터](docs/images/editor-overview.png)

## 할 수 있는 일

- **프로젝트**: 프로젝트 허브와 `besfa` CLI가 Bevy 게임 프로젝트를 만들고 엽니다. Bevy는 한 번 미리 빌드해 함께 쓰므로 게임은 자기 코드만 컴파일합니다.
- **에디터 안에서 실행**: 게임은 뷰포트에서 멈춘 편집 상태로 뜨고, Play로 실행하고 Stop으로 돌아옵니다. 코드를 고치면 다시 빌드합니다.
- **씬 편집**: 씬 엔티티의 Hierarchy, Reflect 컴포넌트를 고치는 Inspector, 카메라·이동/회전/크기 핸들·맞춤이 있는 씬 뷰, 되돌리기. 게임의 씬 파일에 저장합니다.
- **에셋**: glTF 모델과 프리팹 놓기, 썸네일, 바뀐 모델 다시 가져오기, 프리팹 저장과 적용, 모델 하나를 클립과 함께 따로 보는 미리보기.
- **월드 만들기**: 지형 빚기와 칠하기, 폴리지, 붓으로 흩뿌리기, 하늘.
- **애니메이션**: 모델의 클립을 묶는 애니메이션 상태 기계.
- **멀티플레이**: 데디케이트 서버와 거기 들어가는 클라이언트로 Play.
- **에이전트**: `besfa mcp`로 Claude Code가 에디터 없이 게임을 띄워 씬을 고칩니다.

## 구성

에디터와 게임은 따로 도는 프로세스입니다. 에디터는 게임을 `besfa_editor_plugin`과 함께 띄우고, 게임은 에디터가 보여 주는 텍스처에 그리며 stdin과 stdout으로 에디터와 대화합니다. 게임은 이 저장소의 Besfa 크레이트에만 의존하고, 릴리스 빌드에는 에디터 없이 `besfa_components`만 들어갑니다.

| 크레이트 | 역할 |
| --- | --- |
| `besfa_editor` | egui 데스크톱 에디터 |
| `besfa_cli` | `besfa` 명령: 프로젝트 만들기, Bevy 미리 빌드, MCP 서버 띄우기 |
| `besfa_protocol` | 에디터와 게임 사이의 메시지 |
| `besfa_editor_plugin` | 에디터가 게임을 띄울 때 넣는 Bevy 플러그인 |
| `besfa_components` | 게임이 릴리스에 담는 컴포넌트: 씬 파일, 지형, 하늘, 애니메이션 상태 기계 |
| `besfa_net` | 데디케이트 서버와 그 연결 |
| `besfa_mcp` | 에이전트용 MCP 서버 |

## 문서

- [사용 설명서](docs/manual/README.md): 에디터 쓰는 법
- [프로젝트 개요](docs/PROJECT_OVERVIEW.md), [아키텍처](docs/ARCHITECTURE.md)와 [docs](docs)의 다른 설계 문서: 만든 방식
