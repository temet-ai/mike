UPDATE steering.steerers
   SET pattern = E' done\n done.\n done,\ntests pass\ntest pass\n verified\n complete\n completed\n fixed\n it works\nnow working\nsuccessfully\nthe issue is resolved\nissue resolved\nall tests'
 WHERE name = 'false-done';
