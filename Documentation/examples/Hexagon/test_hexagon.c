#include <stdio.h>

#include "Hexagon.h"

int main() {
    // heap
    Hexagon *heapHexagon = Hexagon_new(12);
    printf("heapHexagon area: %f\n", Hexagon_area(heapHexagon));
    Hexagon_delete(heapHexagon);

    // stack
    // Hexagon stackHexagon = { .sideLength = 13 };
    Hexagon stackHexagon;
    Hexagon_init(&stackHexagon, 13);
    printf("stackHexagon area: %f\n", Hexagon_area(&stackHexagon));
    // no need to delete/free

    return 0;
}
