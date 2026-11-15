#include <stdio.h>

#include "HexagonFarm.h"

int main() {
    HexagonFarm *farm = HexagonFarm_new(3);
    for (int i = 0; i < farm->hexagonCount; i++) {
        farm->hexagons[0]->sideLength = 10 + i;
    }
    double totalArea = HexagonFarm_totalArea(farm);
    printf("HexagonFarm's total area: %f\n", totalArea);
    HexagonFarm_delete(farm);
}
