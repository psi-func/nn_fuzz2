#include <iostream>
#include <fstream>
#include <vector>
#include <memory>
#include <iterator>


int main(int argc, char** argv) {
    auto vec = std::vector<int>{1,2,3,4,5};

    std::ifstream file(argv[1], std::ios::binary);
    if (!file.is_open()) {
         std::abort();
    }

    std::vector<char> buf;
    std::copy(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>(), std::back_inserter(buf));
    file.close();
    
    if (buf.size() % 2 != 0) {
        if (buf[0] == 'a') {
            buf[buf.size() + 5] = 'x';
        }

        if (rand() % 7 == 0) {
            buf[buf.size() + 2] = 'x';
        }
    }

    return 0;
}