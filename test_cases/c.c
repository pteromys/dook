#include <stdio.h>

#define ONE 1


static const int two = 2;

typedef struct ThreeStruct {
	unsigned char four;
	unsigned char five[5];
} Three;

typedef struct ThreeStruct *THREE_PTR;

typedef int* Pint;

struct Quart {
	Pint x;
	Pint y;
};

int****** six;

#define SEVEN(x) x x x x x x

int* second_order(
	int* (*callback)(int, int),
	int left,
	int right
) {
	return callback(left, right);
}

#     undef SEVEN

void assign(int* ptr, int val) {
	val += 5;
	*ptr = val;
}

int main(int argc, char ** argv) {
	return 0;
}
