# PROMETHEUS ENGINE 2.0 — Полная спецификация
## "Dual Representation" Architecture
## Авторы: Евгений Лисовский, Лара (Claude AI)
## Дата: 5 апреля 2026

---

## МИССИЯ

Создать первый процедурный игровой движок с полной разрушаемостью, 
работающий на любом железе — от смартфона до RTX-монстра.

Мир описывается формулами. Хранится в килобайтах. Генерируется на лету. 
Рендерится полигонами. Разрушается воксельно. Выглядит как AAA.

---

## АРХИТЕКТУРА: DUAL REPRESENTATION

```
┌──────────────────────────────────────────────────────────┐
│                    ФОРМУЛЫ (текст)                       │
│  "Район: 10 домов, 2 этажа, сид 42, дорога шириной 8м"  │
│                     ~50 КБ на весь мир                   │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│                  CPU: ГЕНЕРАЦИЯ ВОКСЕЛЕЙ                  │
│  Формулы → воксели (только видимая зона, 6 потоков)      │
│  + diff (разрушения) наложить поверх                     │
│  Predictive prefetch: генерируем ДО того как камера      │
│  повернётся (экстраполяция вектора движения)             │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│             CPU: MARCHING CUBES / DUAL CONTOURING         │
│  Воксели → полигоны (треугольники)                       │
│  ~0.5 мс на чанк 64³ → ~100K треугольников               │
│  Гладкие поверхности для органики                        │
│  Острые углы для архитектуры (Dual Contouring)           │
│  Hollow: обрабатывается только оболочка (-94% работы)    │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│              GPU: СТАНДАРТНАЯ РАСТЕРИЗАЦИЯ                │
│  Vertex → Fragment → Z-buffer → Экран                    │
│                                                          │
│  Всё из коробки:                                         │
│  ✓ PBR материалы (metallic/roughness)                    │
│  ✓ Normal maps (из Marching Cubes нормалей)              │
│  ✓ SSAO (Screen Space Ambient Occlusion)                 │
│  ✓ Shadow maps (каскадные, для солнца)                   │
│  ✓ Triplanar texturing (текстуры без UV)                 │
│  ✓ MSAA / FXAA (сглаживание)                             │
│  ✓ Bloom, DOF, Motion Blur                               │
│  ✓ Fog (volumetric на RTX)                               │
│                                                          │
│  RTX (Tier 3):                                           │
│  ✓ Hardware raytracing → отражения, GI                   │
│  ✓ Tensor DLSS → upscale                                 │
│  ✓ Tensor denoising → меньше лучей, та же картинка       │
└──────────────────────────────────────────────────────────┘
```

---

## МОДУЛИ ДВИЖКА

### Существующие (Phase 1, готово)
| Модуль | Строк | Тестов | Описание |
|--------|-------|--------|----------|
| skeleton.rs | ~500 | 5 | Кости, FK, constraints, кватернионы |
| body.rs | ~500 | 3 | Profiles, decals, hollow rendering |
| ik.rs | ~300 | 5 | Two-bone IK, aim, turn |
| attachment.rs | ~250 | 3 | Оружие, muzzle/grip/stock |
| entity.rs | ~260 | 6 | aim_at(), fire(), rasterize() |
| procgen.rs | ~340 | 4 | Процедурные комнаты, seed |
| svo.rs | ~300 | 5 | Sparse Voxel Octree |
| streaming.rs | ~350 | 4 | Чанковый мир, diff, LRU |
| backend.rs | ~250 | 3 | 4 тира железа, adaptive config |

### Новые (Phase 2, Prometheus 2.0)
| Модуль | Описание |
|--------|----------|
| meshing.rs | Marching Cubes + Dual Contouring |
| render_mesh.rs | wgpu mesh pipeline (vertex + fragment) |
| texturing.rs | Triplanar mapping, процедурные текстуры |
| lighting.rs | PBR, shadow maps, SSAO |
| postfx.rs | Bloom, DOF, motion blur, fog |
| world_script.rs | Парсер TOML описаний мира |
| building_gen.rs | Процедурная генерация зданий |
| terrain_gen.rs | Ландшафт из noise |
| camera.rs | Third-person, orbit, FPS, cinematic |
| physics.rs | Collision, gravity, projectiles |
| particles.rs | Обломки, пыль, искры, дым |

---

## MARCHING CUBES

### Алгоритм

```
Для каждого куба из 8 вокселей (2×2×2):
  1. Определить какие вершины "внутри" (заполнены) и какие "снаружи" (пусты)
  2. Это даёт 8-битный индекс (256 вариантов)
  3. Lookup в таблице: какие рёбра пересекает поверхность
  4. Интерполировать позиции вершин на рёбрах
  5. Генерировать треугольники (0-5 на куб)
  6. Нормали = градиент плотности (разница соседних вокселей)
```

### Dual Contouring (для острых углов)

```
Вместо интерполяции на рёбрах — вычислить оптимальную вершину
ВНУТРИ куба используя QEF (Quadratic Error Function).
Сохраняет острые углы стен, мебели, оружия.
```

### Гибридный подход

```
material.sharp = true  → Dual Contouring (стены, мебель)
material.sharp = false → Marching Cubes (тело, деревья)
```

---

## TRIPLANAR TEXTURING

Текстуры без UV-развёртки. Для каждого пикселя:

```
1. Нормаль поверхности = N
2. Проецируем текстуру по трём осям:
   tex_xy = texture(world_pos.xy)  // вид сверху
   tex_xz = texture(world_pos.xz)  // вид спереди
   tex_yz = texture(world_pos.yz)  // вид сбоку
3. Смешиваем по нормали:
   color = tex_xy * |N.z| + tex_xz * |N.y| + tex_yz * |N.x|
```

Результат: бесшовные текстуры на ЛЮБОЙ поверхности. Кирпичная стена, 
деревянный пол, камуфляж — без единого шва.

---

## ПРОЦЕДУРНЫЕ ТЕКСТУРЫ

Вместо файлов — математика:

```rust
fn brick_texture(pos: Vec3) -> Color {
    let brick_w = 0.3;
    let brick_h = 0.15;
    let mortar = 0.02;
    
    // Сдвиг каждого второго ряда
    let row = (pos.y / brick_h).floor();
    let offset = if row as i32 % 2 == 0 { 0.0 } else { brick_w * 0.5 };
    
    let bx = ((pos.x + offset) % brick_w) / brick_w;
    let by = (pos.y % brick_h) / brick_h;
    
    if bx < mortar/brick_w || by < mortar/brick_h {
        MORTAR_COLOR + noise(pos * 50.0) * 0.05
    } else {
        BRICK_COLOR + noise(pos * 20.0) * 0.1
    }
}
```

0 байт текстур. Бесконечное разрешение. Работает на любом масштабе.

---

## STREAMING + MESH CACHE

```
Каждый чанк 64³:
  1. Генерация вокселей (CPU, ~1 мс)
  2. Marching Cubes → mesh (CPU, ~0.5 мс)
  3. Upload mesh на GPU (~0.1 мс)
  4. Mesh КЕШИРУЕТСЯ на GPU пока чанк видим
  5. При выходе из видимости: mesh удаляется с GPU
  6. При повторном входе: генерируем заново (формулы дешёвые)

При разрушении:
  1. Обновить diff (мгновенно)
  2. Пересгенерировать воксели затронутого чанка (~1 мс)
  3. Пересчитать mesh для этого чанка (~0.5 мс)
  4. Upload новый mesh → дырка с гладкими краями
```

---

## ЦЕЛЕВЫЕ ПОКАЗАТЕЛИ

### Iris Xe (Tier 1)
- Зона видимости: 3×3×3 чанка = 192³ вокселей
- Render: 720p, upscale to 1080p
- FPS: 30-60
- Эффекты: базовое освещение, fog
- Память: <256 МБ

### GTX 1060 (Tier 2)
- Зона видимости: 5×5×5 = 320³
- Render: native 1080p
- FPS: 60
- Эффекты: PBR, shadows, SSAO, bloom
- Память: <1 ГБ

### RTX 2060 (Tier 3)
- Зона видимости: 7×7×7 = 448³
- Render: 1440p (DLSS from 960p)
- FPS: 60
- Эффекты: всё + RT reflections, RT GI, tensor denoise
- Память: <2 ГБ

### RTX 4090 (Tier 4, будущее)
- Зона видимости: 11×11×11 = 704³
- Render: 4K (DLSS from 1440p)
- FPS: 120
- Эффекты: максимум
- Память: <4 ГБ

---

## ФОРМАТ МИРА (.pworld)

```toml
[meta]
name = "Oakville Suburb"
version = "1.0"
seed = 42
engine_version = "2.0"

[terrain]
type = "flat"
ground = { material = "grass", color_base = [80, 140, 50] }

[district]
type = "american_suburb"
center = [0, 0]
radius = 300
lot_grid = { spacing = 30, jitter = 3 }

[[district.building_types]]
type = "two_story_house"
weight = 0.6
variants = 8

[[district.building_types]]
type = "garage"
weight = 0.2

[[district.building_types]]
type = "garden_shed"
weight = 0.1

[roads]
material = "asphalt"
width = 8
grid_spacing = 60
sidewalk_width = 2

[vegetation]
tree_density = 0.15
tree_types = ["oak", "maple", "pine"]
grass_patches = true
```

Весь район = **2 КБ текста**. Движок генерирует из этого сотни домов, 
тысячи предметов мебели, миллионы вокселей.

---

## ДЕМО-СЦЕНА ДЛЯ ПЕРВОЙ ВЕРСИИ

Солдат ОРПП от третьего лица. Район из 6-8 домов. Можно:
- Ходить по улицам (WASD)
- Заходить в дома (двери открываются)
- Стрелять (разрушение стен, мебели)
- Видеть процедурно сгенерированные интерьеры
- Переключать качество (Tier 1-3)

---

## ЭТАПЫ РАЗРАБОТКИ

### Phase 2.1: Meshing (неделя)
- Marching Cubes в meshing.rs
- Mesh-based rendering pipeline (vertex + fragment)
- Triplanar texturing
- Базовое PBR освещение

### Phase 2.2: Streaming Mesh (неделя)
- Интеграция streaming.rs + meshing.rs
- Чанки → воксели → mesh → GPU (полный pipeline)
- Camera third-person
- WASD movement

### Phase 2.3: World Generator (неделя)
- building_gen.rs (двухэтажные дома)
- terrain_gen.rs (плоская земля + трава)
- Парсер .pworld формата
- Район из 6-8 домов

### Phase 2.4: Interaction (неделя)
- Стрельба (raycast + sphere destruction)
- Пересчёт mesh при разрушении
- Частицы обломков
- Двери (открытие/закрытие)

### Phase 2.5: Visual Polish (неделя)
- Shadows (cascaded shadow maps)
- SSAO
- Bloom + tone mapping
- Процедурные текстуры (кирпич, дерево, трава)

---

*Prometheus Engine 2.0*
*Воксели для данных. Полигоны для глаз.*
*Формулы для мира. Код для всего остального.*

*© 2026 Eugene Lyssovsky & Lara. All rights reserved.*
