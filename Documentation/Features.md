# Features

'Easier Safer c' promotes using c in a particular way to make it easier to understand, and safer to use.

Outstanding questions,
- what debugging looks like

## 1. Each `.bam` file becomes a `.h`/`.c` 'object' pair backed by a struct

### Example 1
When `bam` sees the file `Hexagon.bam` with this content;
```bam
double sideLength;

void init(double sideLength) {
    self->sideLength = sideLength;
}

double area() {
    double threeTimesSqrtOfThreeAllOverTwo = 2.59807621135;
    double area = threeTimesSqrtOfThreeAllOverTwo * self->sideLength * self->sideLength;
    return area;
}
```
it generates the examples below in Feature #2.

## 2. Keep initialization separate from allocation

The generated `init` method initializes caller-provided storage. This allows an object to be stack-allocated:

```c
Hexagon hexagon;
Hexagon_init(&hexagon, 3.0);
```

The generated heap allocator uses the same initializer and parameters:

```c
Hexagon *hexagon = Hexagon_new(3.0);
if (hexagon == NULL) {
    /* Handle allocation failure. */
}
```

Generation rules:

- Generate `Type_init(Type *self, ...)` from the `.bam` `init` method, preserving its parameters. It must not allocate or free `self`.
- If an `init` method exists, generate `Type_new(...)` with the same parameters. It allocates storage, calls `Type_init` with those parameters, and returns the object. It returns `NULL` if allocation fails.
- If no `init` method exists, generate a no-argument `Type_new(void)` only when zero-initialized storage is defined to be a valid object state.
- Do not generate a second no-argument allocator when an initializer requires parameters. This avoids returning an object that has not been initialized for use.
- Additional creation patterns can be expressed as explicitly named factory methods, rather than overloaded constructors (which C does not support).

It sees one attribute/member variable called `sideLength`, and one method/function called `area`. It should create the following files,

`Hexagon.h`;
```c
#ifndef BAM_GENERATED_HEXAGON_H
#define BAM_GENERATED_HEXAGON_H

typedef struct Hexagon_struct {
    double sideLength;
} Hexagon;

Hexagon *Hexagon_new(double sideLength);
void Hexagon_init(Hexagon *self, double sideLength);
void Hexagon_delete(Hexagon *self);

double Hexagon_area(Hexagon *self);

#endif  // BAM_GENERATED_HEXAGON_H
```

and `Hexagon.c`;
```c
#include <stdlib.h> // malloc, free

#include "Hexagon.h"

Hexagon *Hexagon_new(double sideLength) {
    Hexagon *self = malloc(sizeof(Hexagon));
    if (self == NULL) {
        return NULL;
    }
    Hexagon_init(self, sideLength);
    return self;
}

void Hexagon_init(Hexagon *self, double sideLength) {
    self->sideLength = sideLength;
}

void Hexagon_delete(Hexagon *self) {
    free(self);
}

double Hexagon_area(Hexagon *self) {
    double threeTimesSqrtOfThreeAllOverTwo = 2.59807621135;
    double area = threeTimesSqrtOfThreeAllOverTwo * self->sideLength * self->sideLength;
    return area;
}
```

## 3. Separate resource cleanup from object deletion

An object can own resources that need cleanup, such as the `Hexagon` array in `HexagonFarm`. Generate two lifecycle functions:

- `Type_deinit(Type *self)` releases resources owned by the object and calls `deinit` on owned objects as needed. It does not free `self`, so it can be used with stack-allocated objects.
- `Type_delete(Type *self)` calls `Type_deinit(self)` and then frees `self`. Use it for objects returned by `Type_new`.

For example, a heap-allocated farm is released with `HexagonFarm_delete(farm)`. A stack-allocated farm is cleaned up with `HexagonFarm_deinit(&farm)`:

```c
HexagonFarm stackFarm;
HexagonFarm_init(&stackFarm, 3);
/* Use stackFarm. */
HexagonFarm_deinit(&stackFarm);

HexagonFarm *heapFarm = HexagonFarm_new(3);
if (heapFarm != NULL) {
    /* Use heapFarm. */
    HexagonFarm_delete(heapFarm);
}
```

The `.bam` `deinit` method describes cleanup of resources owned by the object. The generated `delete` method provides the heap-object convenience that pairs deinitialization with freeing the object itself.
