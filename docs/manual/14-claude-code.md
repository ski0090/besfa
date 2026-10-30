[← 사용 설명서 목차](README.md)

# Claude Code로 다루기

[Claude Code](https://claude.com/claude-code)에게 말로 씬 작업을 맡길 수 있습니다. Claude Code가 에디터 없이 게임을 띄워, 씬에 물건을 놓고, 땅을 빚고, 흩뿌리고, 애니메이션 상태 기계를 짜고, 씬 사진을 찍습니다.

## 준비

`besfa new`(또는 허브의 **New project**)로 만든 프로젝트에는 이미 `.mcp.json` 파일이 있습니다. 그 프로젝트 폴더에서 Claude Code를 열면 바로 씁니다.

예전에 만든 프로젝트라면, 프로젝트 폴더에 `.mcp.json` 파일을 만들고 아래 내용을 넣으세요.

```json
{ "mcpServers": { "besfa": { "command": "besfa", "args": ["mcp"] } } }
```

`besfa` 명령이 설치되어 있어야 합니다([설치와 시작](01-getting-started.md#설치)).

## 쓰는 법

프로젝트 폴더에서 Claude Code를 열고 하고 싶은 일을 말하면 됩니다. 예:

- "씬에 있는 엔티티를 보여 줘"
- "`assets/models/oak-tree.glb`를 원점 근처에 다섯 그루 놓아 줘"
- "지형을 만들고 가운데를 언덕으로 올려 줘"
- "zombie 모델에 idle·walk 상태 기계를 붙여 줘"
- "씬 사진을 찍어 줘"

Claude Code는 `besfa mcp`로 게임을 **창 없이** 편집 상태로 띄우고, 에디터가 쓰는 것과 같은 명령을 게임에 보냅니다. 화면을 띄우지 않으므로 작업 중인 다른 창을 가리지 않습니다.

## 주의

- **에디터와 Claude Code가 같은 프로젝트를 동시에 다루지 마세요.** 둘 다 씬 파일을 쓰므로 한쪽의 변경이 다른 쪽에 덮어쓰입니다. 하나를 닫고 다른 쪽을 쓰세요.
- Claude Code가 쓰는 `besfa mcp`는 CLI를 설치할 때 알던 명령만 받고, 모르는 명령은 거절합니다. 게임의 플러그인을 업데이트해 새 명령이 생겼다면 CLI도 다시 설치하세요([업데이트](13-updating.md#에디터와-cli-업데이트)).

---

다음: [단축키](15-shortcuts.md)
