# 하늘 편집 기획

씬의 하늘을 에디터 전용 창에서 고치고 씬 파일에 저장한다. 하늘은 `besfa_editor_plugin`이 제공하는 Reflect 컴포넌트 하나로, 게임 코드가 실행 중에 값을 써도 화면이 따라간다(예: Duskhold의 하루 시계가 시각에 따라 하늘색을 바꾼다).

## 목표

- 하늘은 씬 엔티티의 컴포넌트 `besfa_editor_plugin::sky::Sky`다. 다른 컴포넌트처럼 `.scn.ron`에 저장되고, Inspector·되돌리기·`besfa_mcp`의 `set`이 그대로 통한다.
- 게임 코드는 `Query<&mut Sky>`로 값을 바꾼다. 플러그인이 바뀐 값을 매 프레임 화면에 반영하므로 게임은 렌더링을 몰라도 된다. 편집 상태와 실행 상태, 에디터 없이 실행한 게임 모두 같다.
- 에디터의 Window > Sky가 오른쪽 패널의 Sky 탭을 연다. 색이나 이미지를 고치면 편집 상태의 뷰포트에 바로 보이고, Save scene이 씬에 남긴다.
- Bevy 0.20 밖의 새 크레이트를 더하지 않는다.

## 핵심 결정: 그라데이션과 큐브맵 둘 다, 하나의 경로로

| 방식 | 장점 | 단점 |
| --- | --- | --- |
| 그라데이션(절차적) | 색 세 개라 게임이 시각에 따라 보간하기 쉽다. 에셋이 필요 없다 | 구름·별 같은 무늬가 없다 |
| 큐브맵 이미지 | 그린 하늘·HDR 사진을 그대로 쓴다 | 하루 시계로 색을 바꾸기 어렵다(밝기와 회전만) |

둘 다 지원한다. 두 방식 모두 Bevy 0.20의 `Skybox`(카메라 컴포넌트, 큐브맵 이미지)로 그린다. 그라데이션은 플러그인이 CPU에서 작은 큐브맵(면마다 64×64)을 만들어 같은 `Skybox`에 넣으므로, 셰이더를 새로 쓰지 않고 무한히 먼 배경·깊이 처리·노출을 Bevy가 맡는다. 값이 바뀐 프레임에만 다시 만든다(64×64×6 픽셀, 하루 시계가 매 프레임 바꿔도 가볍다).

절차적 대기 산란(Bevy의 `Atmosphere`)은 해 방향으로 하늘색이 정해져 "하늘색을 직접 고른다"는 요구와 맞지 않고, 카메라마다 무거운 설정이 필요해 이번에는 하지 않는다.

## 컴포넌트

```rust
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct Sky {
    pub mode: SkyMode,        // Gradient | Cubemap
    pub zenith: Color,        // 머리 위
    pub horizon: Color,       // 지평선
    pub ground: Color,        // 지평선 아래
    pub blend: f32,           // 지평선 색이 위로 퍼지는 정도(지수, 작을수록 천정 색이 넓다)
    pub cubemap: String,      // 에셋 폴더 기준 이미지 경로
    pub brightness: f32,      // 1이면 고른 색 그대로(카메라 노출을 상쇄한다)
    pub rotation: f32,        // 하늘을 위에서 보아 반시계로 돌리는 각도(도)
}
```

- 모드를 바꿔도 다른 모드의 값은 남는다. 게임이 모드를 바꿔 낮은 그라데이션, 밤은 별 큐브맵처럼 쓸 수도 있다.
- 그라데이션 색: 방향의 높이 `y`(-1~1)가 0 이상이면 `horizon`에서 `zenith`로 `y^blend`만큼, 0 아래면 `horizon`에서 `ground`로 `min(1, -4y)`만큼 선형 색 공간에서 섞는다.
- 큐브맵: Bevy가 읽는 큐브맵(`.ktx2`, `.dds`)이나 여섯 면(+X, -X, +Y, -Y, +Z, -Z)을 세로로 쌓은 PNG·JPG(높이 = 너비 × 6). 쌓은 이미지는 플러그인이 큐브맵으로 바꿔 읽는다. 아직 읽는 중이거나 읽지 못하면 그라데이션을 보이고, 실패는 로그에 남긴다.
- 씬에 `Sky`가 여럿이면 처음 찾은 것을 쓴다. `Sky`가 없으면 플러그인은 아무것도 하지 않는다(지금처럼 `ClearColor`).

## 플러그인 동작

- `SkyPlugin`은 편집 모드와 상관없이 항상 들어간다(`default_plugins`).
- `Sky`가 바뀌거나 카메라가 새로 생기면 모든 3D 카메라에 `Skybox`를 넣는다. 밝기는 `brightness ÷ 카메라 노출`이라 노출이 다른 편집용 카메라와 게임 카메라에서 같은 색으로 보인다. 플러그인이 쓰는 미리보기·썸네일 카메라와, 게임이 직접 `Skybox`를 붙인 카메라는 건드리지 않는다.
- 플러그인이 넣은 `Skybox`는 표식(`SkyApplied`)과 함께 씬 파일에서 뺀다. `Sky`를 떼면 넣은 `Skybox`도 뗀다.
- Hierarchy의 + 메뉴와 Sky 탭의 Add sky가 `spawn`의 `kind: sky`로 `Name("Sky")`와 기본 `Sky`를 가진 엔티티를 만든다.

## 프로토콜(버전 10)

- `SpawnKind::Sky`.
- `EntityInfo`에 `sky: bool`(기본 false, 거짓이면 쓰지 않는다): 에디터가 선택과 상관없이 하늘 엔티티를 찾는다.
- 값 고치기는 새 명령 없이 기존 `set`을 쓴다.

## 에디터 Sky 탭

- 오른쪽 패널에 Inspector, Scatter, Animation 옆의 Sky 탭. Window > Sky로도 연다.
- 탭을 열면 씬의 하늘 엔티티를 선택해(그래야 게임이 값을 보고한다) 그 `Sky`를 보여 준다. 하늘이 없으면 Add sky 버튼, 플러그인이 버전 10보다 오래되면 갱신하라는 안내.
- Gradient·Cubemap 고르기, 모드별로 Zenith·Horizon·Ground 색 버튼과 Blend 막대, 또는 이미지 경로 칸(Assets 탭에서 이미지를 끌어 놓을 수 있다). 공통으로 Brightness, Rotation.
- 바꿀 때마다 컴포넌트 전체를 `set`으로 보낸다. Inspector의 변경처럼 되돌리기와 저장하지 않은 변경 표시에 들어가고, 실행 상태에서 바꾼 것은 Stop하면 사라진다. 실행 상태에서는 게임이 바꾸는 값을 따라 보여 준다.

## 게임에서 쓰기

```rust
use besfa_editor_plugin::{Sky, SkyMode};

fn day_sky(clock: Res<DayClock>, mut skies: Query<&mut Sky>) {
    for mut sky in &mut skies {
        sky.zenith = clock.zenith_color();
        sky.horizon = clock.horizon_color();
    }
}
```

## 하지 않는 것(후속)

- 하늘이 장면 조명(환경광, `EnvironmentMapLight`)에 주는 영향. 지금은 배경만 그린다.
- 해·달 원반, 구름, 별 같은 절차적 무늬.
- HDR(`.hdr`/`.exr`) 정거 원통 이미지를 큐브맵으로 바꾸기.

## 테스트

- 플러그인: 그라데이션 색 계산, 큐브맵 이미지 생성, 카메라에 `Skybox`가 들어가고 값을 바꾸면 따라가며 `Sky`를 떼면 빠지는지, 씬 저장·읽기 왕복에서 `Sky`가 남고 `Skybox`는 빠지는지.
- 프로토콜: `SpawnKind::Sky`, `EntityInfo.sky`의 직렬화와 옛 보고와의 호환.
- 에디터: Sky 탭이 하늘이 없으면 Add sky를, 있으면 색을 보여 주고 바꾼 값을 `set`으로 내는지.
