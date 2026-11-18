#ifndef BAM_GENERATED_HEXAGONFARM_H
#define BAM_GENERATED_HEXAGONFARM_H

#include "../Hexagon/Hexagon.h"

typedef struct HexagonFarm_struct {
    int hexagonCount;
    Hexagon **hexagons;
} HexagonFarm;

HexagonFarm *HexagonFarm_new(int hexagonCount);
void HexagonFarm_init(HexagonFarm *self, int hexagonCount);
void HexagonFarm_delete(HexagonFarm *self);
void HexagonFarm_deinit(HexagonFarm *self);

double HexagonFarm_totalArea(HexagonFarm *self);

#endif  // BAM_GENERATED_HEXAGONFARM_H
