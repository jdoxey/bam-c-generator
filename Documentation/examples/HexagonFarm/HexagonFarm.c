#include <stdlib.h> // malloc, free

#include "HexagonFarm.h"

HexagonFarm *HexagonFarm_new(int hexagonCount) {
    HexagonFarm *self = malloc(sizeof(HexagonFarm));
    if (self == NULL) {
        return NULL;
    }
    HexagonFarm_init(self, hexagonCount);
    return self;
}

void HexagonFarm_init(HexagonFarm *self, int hexagonCount) {
    self->hexagonCount = hexagonCount;
    self->hexagons = malloc(sizeof(Hexagon *) * hexagonCount);
    for (int i = 0; i < hexagonCount; i++) {
        self->hexagons[i] = Hexagon_new(0);
    }
}

void HexagonFarm_delete(HexagonFarm *self) {
    HexagonFarm_deinit(self);
    free(self);
}

void HexagonFarm_deinit(HexagonFarm *self) {
    for (int i = 0; i < self->hexagonCount; i++) {
        Hexagon_delete(self->hexagons[i]);
    }
    free(self->hexagons);
}

double HexagonFarm_totalArea(HexagonFarm *self) {
    double runningTotal = 0;
    for (int i = 0; i < self->hexagonCount; i++) {
        runningTotal += Hexagon_area(self->hexagons[i]);
    }
    return runningTotal;
}
