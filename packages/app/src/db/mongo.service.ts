import { Injectable } from '@nestjs/common'
import { InjectModel } from '@nestjs/mongoose'
import { Model } from 'mongoose'
import { Fusion } from '@/db/schemas/fusion.schema'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'

@Injectable()
export class MongoService {
    constructor(@InjectModel(Fusion.name) private fusionModel: Model<Fusion>) {}

    async createFusion(fusion: CreateFusionDto) {
        const createdFusion = new this.fusionModel(fusion)
        return createdFusion.save()
    }
}
