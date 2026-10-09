-- Requêtes « pipelinées » : `wrk -s pipeline.lua URL -- 16` en envoie 16 d'un coup.
init = function(args)
  local depth = tonumber(args[1]) or 1
  local r = {}
  for i = 1, depth do r[i] = wrk.format() end
  req = table.concat(r)
end
request = function() return req end
