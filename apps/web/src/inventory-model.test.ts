import {describe,expect,test} from "bun:test";
import {exactBaseQuantity,scaledQuantity} from "./inventory-model";
describe("Konversi inventori yang tepat",()=>{
 test("mengonversi satuan tanpa floating point atau pembulatan tersembunyi",()=>{expect(exactBaseQuantity("2","12","1")).toBe("24");expect(exactBaseQuantity("0.003","1","3")).toBe("0.001");expect(exactBaseQuantity("1.125","8","1")).toBe("9");expect(exactBaseQuantity("0.1","1","1")).toBe("0.1");});
 test("menolak pecahan berulang, nol, negatif, skala dan jumlah berlebih",()=>{for(const value of ["0","-1","1.0001","1000000001","NaN"]){expect(()=>scaledQuantity(value)).toThrow();}expect(()=>exactBaseQuantity("1","1","3")).toThrow();expect(()=>exactBaseQuantity("1000000000","2","1")).toThrow();expect(()=>exactBaseQuantity("1","1","0")).toThrow();});
});
