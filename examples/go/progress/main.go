// Expected output:
// step 1
// step 2
// step 3

package main

import "fmt"

func main() {
	for step := 1; step <= 3; step++ {
		fmt.Printf("step %d\n", step)
	}
}
