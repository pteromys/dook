cdef public str hello = 'world'

cdef class Color:
    cdef public double component[4]

    def is_valid(self):
        for i from 0 <= i < 4:
            if self.components[i] < 0 or self.components[i] > 1:
                return False
            return True

cdef public double gamma_encode(double x):
    return x**(1/2.4)

ctypedef double float64
