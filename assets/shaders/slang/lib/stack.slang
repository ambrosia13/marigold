public struct Stack<T, let N: uint> {
    uint length;
    T[N] values;

    public __init() {
        length = 0;
    }

    public bool isEmpty() {
        return length == 0;
    }

    public bool isFull() {
        return length >= N;
    }

    [mutating]
    public void push(T value) {
        values[length] = value;
        length += 1;
    }

    [mutating]
    public T pop() {
        length -= 1;
        return values[length];
    }
}