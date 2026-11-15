text_hexagon_farm: HexagonFarm.o Hexagon.o test_hexagon_farm.c
	cc -o test_hexagon_farm test_hexagon_farm.c HexagonFarm.o Hexagon.o

HexagonFarm.o: HexagonFarm.c HexagonFarm.h Hexagon.o
	cc -o HexagonFarm.o -c HexagonFarm.c

Hexagon.o: ../Hexagon/Hexagon.c ../Hexagon/Hexagon.h
	cc -o Hexagon.o -c ../Hexagon/Hexagon.c

@phony clean:
	rm -f *.o
	rm -f test_hexagon_farm
