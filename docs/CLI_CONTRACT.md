# CLI 계약

이 문서는 Flutter UI와 `besfa_cli`가 프로젝트 생성 시 사용할 초기 기계 판독 계약의 초안이다. 실제 구현 전에 검토하고 확정한다.

생성은 Cargo의 바이너리 프로젝트 생성을 기반으로 하고, 그 위에 하나로 고정된 Bevy 시작 코드를 덧씌운다. 선택 가능한 템플릿 기능은 실제로 제작·검증한 프로젝트에서 재사용 가능한 부분을 추출한 뒤 후속 단계에서 추가한다.

## 명령

```text
besfa new <DIRECTORY> --output json
```

`DIRECTORY`는 생성될 게임 프로젝트의 최종 루트 디렉터리다.

`besfa new`는 임시 디렉터리에서 다음에 준하는 Cargo 명령을 실행해 바이너리 Rust 프로젝트를 만든다.

```text
cargo new --bin <프로젝트 이름>
```

프로젝트 이름은 `DIRECTORY`의 마지막 경로 구성 요소에서 결정한다. Cargo가 프로젝트를 만든 뒤, `besfa`는 내장 Bevy 시작 코드를 적용한다.

- `Cargo.toml`의 `[dependencies]`에 `bevy = "0.19.1"`과 `besfa_editor_plugin`(이 저장소의 git 의존성)을 추가하고, Bevy 권장 `dev` 프로필 최적화 설정을 붙인다.
- `src/main.rs`를 씬 파일을 읽고 큐브를 돌리는 게임으로 바꾼다. 원본은 `crates/besfa_cli/template/main.rs`다.
- `assets/scenes/main.scn.ron`에 카메라, 조명, 회전하는 큐브가 있는 씬을 쓴다. 원본은 `crates/besfa_cli/template/main.scn.ron`이고, `{{crate}}`를 프로젝트의 크레이트 이름(디렉터리 이름의 `-`를 `_`로)으로 바꾼다. 형식은 [DATA_FORMAT.md](./DATA_FORMAT.md)의 '씬 파일'을 따른다.

`%LOCALAPPDATA%`가 있으면 모든 게임이 Bevy 빌드를 공유하도록 두 가지를 더 한다.

- `.cargo/config.toml`에 `target-dir = "%LOCALAPPDATA%/Besfa/target"`을 쓴다.
- `%LOCALAPPDATA%/Besfa/prebuild/Cargo.lock`이 있으면 새 프로젝트로 복사해, 의존성 버전을 미리 빌드된 것과 맞춘다.

```text
besfa prebuild
```

`%LOCALAPPDATA%/Besfa/prebuild`를 `besfa new`와 같은 템플릿의 게임 프로젝트로 만들고(`Cargo.toml`, `src/main.rs`, `.cargo/config.toml`이 템플릿과 다를 때만 다시 써서, 바뀐 것이 없으면 다시 컴파일하지 않는다), 그 안에서 `cargo update -p besfa_editor_plugin`으로 플러그인을 최신 git 커밋으로 올린 뒤(에디터의 Run이 package cache 잠금을 기다리지 않도록 재시도 없이 요청당 10초만 기다리고, 실패하면 경고만 남기고 고정된 버전으로 계속한다) `cargo build --features bevy/dynamic_linking`을 실행한다. 공유 target 디렉터리, 의존성, feature, 프로필이 에디터의 Run과 같으므로, 이후 게임의 첫 Run은 게임 코드만 컴파일한다. 빌드가 만든 `Cargo.lock`은 `besfa new`가 새 프로젝트로 복사한다. Cargo 출력은 그대로 표준 출력·오류로 나간다. `LOCALAPPDATA`가 없으면 종료 코드 `20`, Cargo 실행이나 빌드가 실패하면 `30`을 반환한다.

생성 시점에는 네트워크를 쓰지 않는다. 의존성은 게임을 처음 빌드할 때 받는다. 모든 단계가 성공한 경우에만 결과물을 최종 루트 디렉터리로 이동한다. `besfa/` 또는 `.besfa/` 데이터 경로와 초기 데이터 파일 생성은 데이터 포맷 계약을 확정한 뒤 추가한다.

초기 구현은 Cargo의 기본 VCS 및 Rust edition 설정을 그대로 따른다. 게임 프로젝트의 VCS 설정 선택은 추후 에디터 UI에서 제공한다.

## JSON 출력

`--output json` 모드에서는 표준 출력에 결과 JSON 객체 하나만 출력한다. 표준 오류는 사람이 읽는 진단 정보에만 사용한다.

성공 예시:

```json
{
  "status": "success",
  "project_path": "D:\\Projects\\my_first_game"
}
```

실패 예시:

```json
{
  "status": "error",
  "code": "destination_exists",
  "message": "The destination directory already exists."
}
```

## 종료 코드

| 코드 | 의미 |
| --- | --- |
| `0` | 성공 |
| `2` | 잘못된 CLI 인자 (Clap) |
| `10` | 대상 디렉터리가 이미 존재함 |
| `11` | Cargo가 거부한 유효하지 않은 프로젝트 이름 또는 요청 |
| `20` | 파일 시스템 오류 |
| `30` | 예상하지 못한 내부 오류 |

Cargo가 프로젝트 이름 때문에 생성에 실패하면, `besfa`는 Cargo의 진단을 표준 오류로 전달하고 종료 코드 `11`을 반환한다. UI는 이 진단을 사용자에게 표시할 수 있다.

## 대상 디렉터리 정책

- 대상 디렉터리가 존재하면, 비어 있어도 생성에 실패한다.
- 생성기는 대상과 같은 부모 디렉터리 아래의 임시 디렉터리에서 작업한다.
- 모든 파일 작성과 검증이 성공한 뒤에만 최종 디렉터리로 이동한다.
- 실패한 생성 작업은 임시 디렉터리를 정리한다.

## 템플릿 도입 계획

초기 버전에는 `--template` 옵션을 제공하지 않는다. 먼저 내장 Bevy 시작 코드로 생성한 프로젝트를 실제로 제작하고 테스트한다. 그 결과에서 공통으로 재사용할 수 있고 안정성이 검증된 파일, 설정, 구조만 추출해 템플릿으로 만든다.

템플릿이 도입되는 시점에 다음을 별도 계약 변경으로 추가한다.

- `--template <TEMPLATE>` 옵션
- 템플릿 탐색 및 복사 정책
- 존재하지 않는 템플릿에 대한 종료 코드 `12`

## 구현 상태

`besfa new`는 이 문서의 Cargo 실행, Bevy 시작 코드 적용, JSON 출력, 종료 코드, 대상 디렉터리 정책을 구현하고, `besfa prebuild`는 공유 Bevy 빌드를 구현한다. `besfa validate`는 아직 구현되지 않았다.
